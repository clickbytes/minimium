//! Owner-lane impact lifecycle. Workers return immutable durable acknowledgements.
mod checkpoints;
mod owner;
pub(super) mod projection;
mod receipts;
mod replication;
mod spawn;
use super::*;
use minimium_world::remnants::RemnantSnapshot;
use minimium_world::{
    entity::EntityId,
    impact::{BodyState, BODY_COMPONENT, BODY_SCHEMA},
    world::{SectorKey, WorldInstanceId},
};
pub use receipts::{ImpactReceipt, ImpactReceiptStage};
use std::collections::BTreeMap;

pub(super) struct Impacts {
    pub(super) profile: Option<minimium_world::impact::ImpactProfile>,
    async_work_policy: crate::AsyncWorkPolicy,
    journal: Box<dyn minimium_storage::ImpactJournalPort>,
    bodies: BTreeMap<(WorldInstanceId, u64), BodyState>,
    pending: BTreeMap<(WorldInstanceId, u64), BodyState>,
    pub(super) owner_ports: BTreeMap<WorldInstanceId, ComponentOwnerPort>,
    clients: HashMap<PlayerId, replication::Cursor>,
    integration: Option<Vec<(WorldInstanceId, u64)>>,
    last_manual: Option<Instant>,
    failure: Option<String>,
    receipts: receipts::ImpactReceiptLedger,
}

#[derive(Clone, Copy, Debug)]
pub struct ImpactStatus {
    pub active: usize,
    pub craters: usize,
    pub pending: usize,
    pub ready: bool,
}

impl Impacts {
    pub(super) fn failed(&self) -> bool {
        self.failure.is_some()
    }

    pub(super) fn from_storage(
        journal: Box<dyn minimium_storage::ImpactJournalPort>,
        restored: Vec<BodyState>,
        profile: Option<minimium_world::impact::ImpactProfile>,
        async_work_policy: crate::AsyncWorkPolicy,
    ) -> io::Result<Self> {
        if profile.is_some_and(|p| !p.valid()) {
            return Err(io::Error::other("invalid impact configuration"));
        }
        if restored.len() > minimium_storage::impact_journal::MAX_BODIES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "impact restore exceeds the durable body limit",
            ));
        }
        let mut identities = std::collections::BTreeSet::new();
        if restored
            .iter()
            .any(|body| !body.valid() || !identities.insert((body.world, body.id)))
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "impact restore contains an invalid or duplicate body",
            ));
        }
        Ok(Self {
            profile,
            async_work_policy,
            journal,
            bodies: restored.into_iter().map(|b| ((b.world, b.id), b)).collect(),
            pending: BTreeMap::new(),
            owner_ports: BTreeMap::new(),
            clients: HashMap::new(),
            integration: None,
            last_manual: None,
            failure: None,
            receipts: receipts::ImpactReceiptLedger::default(),
        })
    }
}

impl ServerRuntime {
    pub(crate) fn impact_status_snapshot(&self) -> ImpactStatus {
        ImpactStatus {
            active: self.active_body_count(),
            craters: self.impacts.bodies().filter(|b| b.phase != 0).count(),
            pending: self.impacts.pending_count(),
            ready: self.impacts.failure.is_none(),
        }
    }

    pub(super) fn restore_impacts(&mut self) -> io::Result<()> {
        for (world, owner) in self.game.worlds.iter() {
            for (id, kind, _) in owner.entities.iter_components() {
                if kind == BODY_COMPONENT && !self.impacts.contains_body(&(world, id.get())) {
                    return Err(io::Error::other(
                        "impact journal missing saved body; restore impacts.v2 with the world save",
                    ));
                }
            }
        }
        for body in self.impacts.bodies().copied().collect::<Vec<_>>() {
            if !self.impact_projection_matches(body) {
                return Err(io::Error::new(io::ErrorKind::InvalidData,
                    "impact journal and saved component disagree; restore the matching simulation boundary"));
            }
            self.reserve_body_budget(body)?;
            self.publish_impact_projection(body, true)?;
        }
        Ok(())
    }

    pub(super) fn publish_impact_projection(
        &mut self,
        body: BodyState,
        restoring: bool,
    ) -> io::Result<()> {
        let port = self
            .impact_owner_port(body.world)
            .map_err(io::Error::other)?
            .clone();
        self.component_owner_authority
            .validate(&port, port.owner(), body.world)
            .map_err(io::Error::other)?;
        // Validate every fallible mutation before publishing entity and terrain together.
        let changed = projection::publish(
            &mut self.game.worlds,
            &self.component_registry,
            &self.component_owner_authority,
            &port,
            body,
            restoring,
        )?;
        let owner = self.game.worlds.get(body.world).unwrap();
        if changed && !restoring {
            if let Some(stamp) = body.crater() {
                let manifest = owner.manifest;
                self.game
                    .sectors
                    .invalidate_where(body.world, |key| affected(key, manifest, stamp));
            }
        }
        Ok(())
    }

    pub(super) fn update_impacts(&mut self) -> bool {
        let mut keys = self.impacts.integration.take().unwrap_or_else(|| {
            self.impacts
                .bodies()
                .filter(|body| body.phase == 0)
                .map(|body| (body.world, body.id))
                .collect::<Vec<_>>()
        });
        while let Some(key) = keys.pop() {
            if self.impacts.body(&key).is_none_or(|b| b.phase != 0) {
                continue;
            }
            let mut body = *self
                .impacts
                .body(&key)
                .expect("selected body remains owned");
            let Some(owner) = self.game.worlds.get(body.world) else {
                self.impact_failure(io::Error::other("impact world or terrain unavailable"));
                return false;
            };
            let Ok(geography) = self
                .game
                .worlds
                .prepared_terrain_inputs(body.world, self.prepared_geography.as_ref())
            else {
                self.impact_failure(io::Error::other("impact world or terrain unavailable"));
                return false;
            };
            let environment = Surface {
                can_fragment: self.active_body_count() < minimium_world::impact::MAX_ACTIVE_BODIES
                    && self.impacts.body_count() + self.impacts.pending_count() < 4096,
                manifest: owner.manifest,
                geography,
            };
            let result = minimium_simulation::impact::step_body(
                &mut body,
                self.impacts
                    .async_work_policy
                    .impact_fixed_step()
                    .as_secs_f64(),
                &environment,
            );
            if let Some(mut fragment) = result.fragment {
                let result = self.reserve_body_entity(fragment.world).and_then(|id| {
                    fragment.id = id;
                    fragment.seed ^= id.wrapping_mul(0x9e3779b97f4a7c15);
                    self.queue_body_batch(vec![body, fragment])
                });
                if let Err(error) = result {
                    self.impact_failure(error);
                    return false;
                }
            } else if result.contacted {
                if let Err(error) = self.queue_impact_excavation(body) {
                    if error.kind() == io::ErrorKind::WouldBlock {
                        keys.push(key);
                        self.impacts.integration = Some(keys);
                        return false;
                    } else {
                        self.impact_failure(error);
                        return false;
                    }
                }
            } else if (body.age_s
                / self
                    .impacts
                    .async_work_policy
                    .impact_checkpoint_age()
                    .as_secs_f64())
            .floor()
                > (self
                    .impacts
                    .body(&key)
                    .expect("selected body remains owned")
                    .age_s
                    / self
                        .impacts
                        .async_work_policy
                        .impact_checkpoint_age()
                        .as_secs_f64())
                .floor()
            {
                if let Err(error) = self.queue_body(body) {
                    self.impact_failure(error);
                    return false;
                }
            } else {
                if let Err(reason) = self.impacts.advance_live_body(body) {
                    self.impact_failure(io::Error::other(reason));
                    return false;
                }
            }
            if self.worker_barrier() {
                // Revisit an asynchronous contact after acknowledgement. A stale
                // geometry result leaves the live body at its original age.
                if result.contacted {
                    keys.push(key);
                }
                self.impacts.integration = Some(keys);
                return false;
            }
        }
        true
    }

    fn queue_body(&mut self, body: BodyState) -> io::Result<()> {
        self.queue_body_batch(vec![body])
    }

    fn queue_body_batch(&mut self, batch: Vec<BodyState>) -> io::Result<()> {
        if batch.len() == 1 && self.impact_projection_matches(batch[0]) {
            self.release_unowned_impact_reservations(&batch);
            return Ok(());
        }
        if batch
            .iter()
            .any(|body| self.impact_projection_matches(*body))
        {
            self.release_unowned_impact_reservations(&batch);
            return Err(io::Error::other(
                "impact checkpoint batch repeats a durable body state",
            ));
        }
        let prepared = (|| {
            for body in &batch {
                let port = self
                    .impact_owner_port(body.world)
                    .map_err(io::Error::other)?
                    .clone();
                self.component_owner_authority
                    .validate(&port, port.owner(), body.world)
                    .map_err(io::Error::other)?;
                projection::prepare(
                    &self.game.worlds,
                    &self.component_registry,
                    &self.component_owner_authority,
                    &port,
                    *body,
                    false,
                )?;
                self.reserve_body_budget(*body)?;
            }
            self.impacts.begin_checkpoints(&batch)
        })();
        if let Err(error) = prepared {
            self.release_unowned_impact_reservations(&batch);
            return Err(error);
        }
        if let Err(error) = self.impacts.journal.submit_batch(batch.clone()) {
            for body in &batch {
                self.impacts.discard_pending_checkpoint(body);
            }
            self.release_unowned_impact_reservations(&batch);
            return Err(error);
        }
        Ok(())
    }

    fn active_body_count(&self) -> usize {
        self.impacts.bodies().filter(|b| b.phase == 0).count()
            + self
                .impacts
                .pending
                .keys()
                .filter(|k| !self.impacts.contains_body(k))
                .count()
    }

    pub(super) fn reserve_body_budget(&mut self, body: BodyState) -> io::Result<()> {
        let entity = EntityId::new(body.id).ok_or_else(|| io::Error::other("invalid body ID"))?;
        let blob = minimium_world::entity::ComponentBlob::new(BODY_SCHEMA, body.encode())
            .ok_or_else(|| io::Error::other("invalid body component"))?;
        let budget = self.operations.player_store.snapshot_budget_mut();
        let mut staged = budget.clone();
        let key =
            minimium_storage::SnapshotRecordKey::Component(body.world, entity, BODY_COMPONENT);
        staged.reserve_many([
            (
                minimium_storage::SnapshotRecordKey::Entity(body.world, entity),
                minimium_storage::entity_record_bytes(body.world, entity)?,
            ),
            (
                key,
                minimium_storage::component_record_bytes(
                    body.world,
                    entity,
                    BODY_COMPONENT,
                    &blob,
                )?,
            ),
        ])?;
        staged.reserve_component_payloads([(key, blob.bytes.len())])?;
        *budget = staged;
        Ok(())
    }

    fn impact_projection_matches(&self, body: BodyState) -> bool {
        let Some(entity) = EntityId::new(body.id) else {
            return false;
        };
        self.game
            .worlds
            .get(body.world)
            .and_then(|owner| owner.entities.get(entity, BODY_COMPONENT))
            .and_then(|blob| BodyState::decode(&blob.bytes))
            == Some(body)
    }

    fn reserve_body_entity(&mut self, world: WorldInstanceId) -> io::Result<u64> {
        let owner = self
            .game
            .worlds
            .get(world)
            .ok_or_else(|| io::Error::other("unknown world"))?;
        let id = owner
            .entities
            .next_id()
            .checked_add(1)
            .ok_or_else(|| io::Error::other("entity IDs exhausted"))?;
        let sector = SectorKey::Planet(
            minimium_world::world::PlanetSectorKey::from_direction_with_count(
                [0.0, 1.0, 0.0],
                minimium_procgen::planet_sector_count(owner.manifest),
            ),
        );
        let entity = EntityId::new(id).ok_or_else(|| io::Error::other("invalid impact ID"))?;
        self.reserve_impact_entity(world, entity)
            .map_err(io::Error::other)?;
        let staged_result = (|| {
            let staged = self
                .game
                .worlds
                .stage_entity_effect_batch(
                    &[crate::modules::WorldEffect::SpawnEntity {
                        world,
                        sector,
                        components: Vec::new(),
                    }],
                    &self.component_registry,
                    &self.component_owner_authority,
                    None,
                )
                .map_err(|e| io::Error::other(format!("impact allocation: {e:?}")))?;
            let reservation = self
                .operations
                .player_store
                .snapshot_budget_mut()
                .prepare_entity_changes(staged.changes())
                .map_err(|_| io::Error::other("impact allocation: SnapshotBudgetExceeded"))?;
            staged.commit(reservation);
            Ok(())
        })();
        if let Err(error) = staged_result {
            let _ = self.release_impact_entity(world, entity);
            return Err(error);
        }
        Ok(id)
    }

    pub(super) fn impact_failure(&mut self, error: io::Error) {
        self.impacts.fail(&self.world_logger, error);
    }
}

struct Surface {
    can_fragment: bool,
    manifest: minimium_world::world::WorldManifest,
    geography: minimium_procgen::TerrainInputs,
}
impl minimium_simulation::impact::BodyEnvironment for Surface {
    fn terrain_distance(&self, position: [f64; 3]) -> Option<f64> {
        let terrain =
            minimium_procgen::PlanetTerrain::for_manifest(self.manifest, self.geography.clone())
                .ok()?;
        // Soil depth affects material identity only; this query reads distance.
        let base = terrain.volume_base()?;
        if let Some(store) = self.geography.overlays().volumes() {
            store
                .view(&base)
                .interpolated(position)
                .map(|s| s.distance_m)
        } else {
            minimium_terrain::BaseTerrain::sample(&base, position).map(|s| s.distance_m)
        }
    }

    fn can_fragment(&self) -> bool {
        self.can_fragment
    }
    fn ground_radius(&self, direction: [f64; 3]) -> Option<f64> {
        minimium_procgen::planet_point_at_direction_with_geography(
            self.manifest,
            direction.map(|v| v as f32),
            self.geography.clone(),
        )
        .map(|p| p.height_mm as f64 / 1000.0)
    }
}

fn affected(
    key: SectorKey,
    manifest: minimium_world::world::WorldManifest,
    stamp: minimium_world::terrain_edits::CraterStamp,
) -> bool {
    let SectorKey::Planet(key) = key else {
        return false;
    };
    let count = minimium_procgen::planet_sector_count(manifest);
    let Some(sample) = key.cube_sample_with_count(1, 1, 2, count) else {
        return false;
    };
    let radius = manifest
        .planet()
        .map_or(10000.0, |p| p.radius_mm as f64 / 1000.0);
    let patch = minimium_world::terrain_edits::project(sample.coordinates().map(f64::from), radius);
    let center = minimium_world::terrain_edits::project(stamp.center, radius);
    minimium_world::terrain_edits::length(std::array::from_fn(|i| patch[i] - center[i]))
        <= radius * 4.0 / f64::from(count) + stamp.radius_m * 2.0
}
