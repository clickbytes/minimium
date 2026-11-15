//! Owner-lane impact lifecycle. Workers return immutable durable acknowledgements.
mod owner;
pub(super) mod projection;
pub(crate) use owner::ProtectedImpactEntities;
mod replication;
mod spawn;
use super::*;
use minimium_world::remnants::RemnantSnapshot;
use minimium_world::{
    entity::EntityId,
    impact::{BodyState, BODY_COMPONENT, BODY_SCHEMA},
    world::{SectorKey, WorldInstanceId},
};
use std::collections::BTreeMap;

pub(super) struct Impacts {
    pub(super) profile: Option<minimium_world::impact::ImpactProfile>,
    journal: minimium_storage::impact_journal::ImpactJournal,
    bodies: BTreeMap<(WorldInstanceId, u64), BodyState>,
    pending: BTreeMap<(WorldInstanceId, u64), BodyState>,
    clients: HashMap<PlayerId, replication::Cursor>,
    last_manual: Option<Instant>,
    failure: Option<String>,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct ImpactStatus {
    pub active: usize,
    pub craters: usize,
    pub pending: usize,
    pub ready: bool,
}

impl Impacts {
    pub(super) fn open(
        path: &std::path::Path,
        profile: Option<minimium_world::impact::ImpactProfile>,
    ) -> io::Result<Self> {
        if profile.is_some_and(|p| !p.valid()) {
            return Err(io::Error::other("invalid impact configuration"));
        }
        let (journal, restored) = minimium_storage::impact_journal::ImpactJournal::open(path)?;
        Ok(Self {
            profile,
            journal,
            bodies: restored.into_iter().map(|b| ((b.world, b.id), b)).collect(),
            pending: BTreeMap::new(),
            clients: HashMap::new(),
            last_manual: None,
            failure: None,
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
                        "impact journal missing saved body; restore impacts.v1 with the world save",
                    ));
                }
            }
        }
        for body in self.impacts.bodies().copied().collect::<Vec<_>>() {
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
        // Validate every fallible mutation before publishing entity and terrain together.
        let changed = projection::publish(
            &mut self.game.worlds,
            &self.component_registry,
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

    pub(super) fn update_impacts(&mut self) {
        while let Some(result) = self.impacts.journal.poll() {
            match result {
                Ok(batch) => {
                    if !self.impacts.checkpoints_match(&batch) {
                        self.impact_failure(io::Error::other(
                            "stale impact journal checkpoint batch acknowledgement",
                        ));
                        self.shutdown_request.request();
                        return;
                    }
                    for body in batch {
                        if let Err(error) = self.publish_impact_projection(body, false) {
                            self.impact_failure(error);
                            return;
                        }
                        if let Err(error) = self.impacts.acknowledge_checkpoint(body) {
                            self.impact_failure(error);
                            self.shutdown_request.request();
                            return;
                        }
                    }
                }
                Err(error) => {
                    self.impact_failure(error);
                    return;
                }
            }
        }
        if self.impacts.failure.is_some() {
            self.replicate_impacts();
            return;
        }
        let keys: Vec<_> = self
            .impacts
            .bodies()
            .map(|body| ((body.world, body.id), body))
            .filter(|(k, b)| b.phase == 0 && !self.impacts.contains_pending(k))
            .map(|(k, _)| k)
            .collect();
        for key in keys {
            if self.excavation_reserves(key.0) {
                continue;
            }
            let mut body = *self
                .impacts
                .body(&key)
                .expect("selected body remains owned");
            let Some(owner) = self.game.worlds.get(body.world) else {
                continue;
            };
            let geography = minimium_procgen::TerrainInputs {
                geography: self.prepared_geography.clone(),
                material_profile: Some(owner.terrain_material_profile),
                edits: Some(owner.terrain.clone()),
                remnants: Some(owner.remnants.clone()),
                volumes: owner.volumes.clone(),
            };
            let environment = Surface {
                can_fragment: self.active_body_count() < minimium_world::impact::MAX_ACTIVE_BODIES
                    && self.impacts.body_count() + self.impacts.pending_count() < 4096,
                manifest: owner.manifest,
                geography,
            };
            let result = minimium_simulation::impact::step_body(&mut body, 0.05, &environment);
            if let Some(mut fragment) = result.fragment {
                let result = self.reserve_body_entity(fragment.world).and_then(|id| {
                    fragment.id = id;
                    fragment.seed ^= id.wrapping_mul(0x9e3779b97f4a7c15);
                    self.queue_body_batch(vec![body, fragment])
                });
                if let Err(error) = result {
                    self.impact_failure(error);
                    return;
                }
            } else if result.contacted {
                if let Err(error) = self.queue_impact_excavation(body) {
                    if error.kind() != io::ErrorKind::WouldBlock {
                        self.impact_failure(error);
                        return;
                    }
                }
            } else if (body.age_s / 5.0).floor()
                > (self
                    .impacts
                    .body(&key)
                    .expect("selected body remains owned")
                    .age_s
                    / 5.0)
                    .floor()
            {
                if let Err(error) = self.queue_body(body) {
                    self.impact_failure(error);
                    return;
                }
            } else {
                if let Err(reason) = self.impacts.advance_live_body(body) {
                    self.impact_failure(io::Error::other(reason));
                    return;
                }
            }
        }
        self.replicate_impacts();
    }

    fn queue_body(&mut self, body: BodyState) -> io::Result<()> {
        self.queue_body_batch(vec![body])
    }

    fn queue_body_batch(&mut self, batch: Vec<BodyState>) -> io::Result<()> {
        if batch.len() == 1 && self.impact_projection_matches(batch[0]) {
            return Ok(());
        }
        if batch
            .iter()
            .any(|body| self.impact_projection_matches(*body))
        {
            return Err(io::Error::other(
                "impact checkpoint batch repeats a durable body state",
            ));
        }
        for body in &batch {
            projection::prepare(&self.game.worlds, &self.component_registry, *body, false)?;
            self.reserve_body_budget(*body)?;
        }
        self.impacts.begin_checkpoints(&batch)?;
        if let Err(error) = self.impacts.journal.submit_batch(batch.clone()) {
            for body in &batch {
                self.impacts.discard_pending_checkpoint(body);
            }
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
                &self.impacts.protected_entities(),
            )
            .map_err(|e| io::Error::other(format!("impact allocation: {e:?}")))?;
        let reservation = self
            .operations
            .player_store
            .snapshot_budget_mut()
            .prepare_entity_changes(staged.changes())
            .map_err(|_| io::Error::other("impact allocation: SnapshotBudgetExceeded"))?;
        staged.commit(reservation);
        Ok(id)
    }

    pub(super) fn impact_failure(&mut self, error: io::Error) {
        self.impacts.fail(&self.world_logger, error);
    }

    pub(super) fn shutdown_impacts(&mut self, deadline: Instant) -> io::Result<()> {
        for body in self
            .impacts
            .bodies()
            .filter(|b| b.phase == 0)
            .copied()
            .collect::<Vec<_>>()
        {
            if !self.impacts.contains_pending(&(body.world, body.id))
                && !self.impact_projection_matches(body)
            {
                self.impacts.journal.submit(body)?;
            }
        }
        self.impacts.journal.shutdown_until(deadline)?;
        if let Some(error) = &self.impacts.failure {
            return Err(io::Error::other(error.clone()));
        }
        Ok(())
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
        if let Some(store) = &self.geography.volumes {
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
