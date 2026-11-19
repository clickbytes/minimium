mod interests;

use super::sector_worker::{PreparedScene, SectorWorker};
pub(in crate::renderer) use interests::{SceneInterestOwner, SceneInterestPresentation};
pub(super) use minimium_client_runtime::world_readiness::WorldReadinessError as SectorWorkerError;
use minimium_client_runtime::world_readiness::{CollisionSnapshot, WorldReadiness};
use minimium_procgen::SectorGeometry;
use minimium_world::world::{
    GenerationRevision, SectorKey, WorldContentRevision, WorldInstanceId, WorldManifest, WorldMap,
};
use std::{collections::HashMap, sync::Arc};

/// Presentation consumes the CPU owner's exact immutable sectors.
pub(super) struct ClientSectorCache {
    pub(super) cpu: WorldReadiness,
    pub(super) presentation: SectorPresentationCache,
}

pub(super) struct SectorPresentationCache {
    pub(super) worker: SectorWorker,
    presentations: HashMap<SceneInterestOwner, Arc<PreparedScene>>,
    water_presentations: HashMap<SceneInterestOwner, super::sector_worker::WaterPresentation>,
}

pub(super) struct ClientSectorScene {
    pub(super) owner: SceneInterestOwner,
    pub(super) prepared: Option<Arc<PreparedScene>>,
    pub(super) origin: [f32; 3],
    pub(super) origin_generation: u64,
    pub(super) render_sectors: Vec<(SectorKey, u8, Arc<SectorGeometry>)>,
    pub(super) cpu: CollisionSnapshot,
    pub(super) generation: GenerationRevision,
    pub(super) content_revision: WorldContentRevision,
}

impl ClientSectorCache {
    pub(super) fn shutdown_until(&mut self, deadline: std::time::Instant) -> Vec<&'static str> {
        let mut unfinished = Vec::new();
        if !self.presentation.worker.shutdown_until(deadline) {
            unfinished.push("client-terrain-presentation");
        }
        unfinished.extend(self.cpu.shutdown_until(deadline));
        unfinished
    }

    pub(super) fn presentation(&self, owner: SceneInterestOwner) -> Option<&PreparedScene> {
        self.presentation
            .presentations
            .get(&owner)
            .map(AsRef::as_ref)
    }

    pub(super) fn new() -> Result<Self, SectorWorkerError> {
        Ok(Self {
            cpu: WorldReadiness::new()?,
            presentation: SectorPresentationCache {
                worker: SectorWorker::new()?,
                presentations: HashMap::new(),
                water_presentations: HashMap::new(),
            },
        })
    }

    pub(super) fn ensure_scene(
        &mut self,
        owner: SceneInterestOwner,
        instance: WorldInstanceId,
        manifest: WorldManifest,
        position: [f32; 3],
        view: minimium_renderer::terrain_view::TerrainView,
        origin: [f32; 3],
        origin_generation: u64,
        maps: &HashMap<WorldInstanceId, WorldMap>,
        manifests: &HashMap<WorldInstanceId, WorldManifest>,
        map_override: Option<(&WorldMap, WorldManifest)>,
        geography: minimium_procgen::TerrainInputs,
        water: Option<super::sector_worker::WaterPresentation>,
    ) -> Result<Option<ClientSectorScene>, SectorWorkerError> {
        let world_matches = map_override
            .filter(|_| owner.instance() == instance)
            .map(|(_, candidate)| candidate == manifest)
            .unwrap_or_else(|| {
                maps.get(&instance).is_some() && manifests.get(&instance) == Some(&manifest)
            });
        if !world_matches {
            return Ok(None);
        }
        let source_map = map_override
            .filter(|_| owner.instance() == instance)
            .map(|(map, _)| map)
            .or_else(|| maps.get(&instance));
        let Some(map) = source_map else {
            return Ok(None);
        };
        let Some(cpu) =
            self.cpu
                .ensure_collision(owner, manifest, position, map, geography.clone())?
        else {
            return Ok(None);
        };
        self.presentation
            .prepare_scene(cpu, view, origin, origin_generation, geography, water)
    }

    pub(super) fn remove_world_data(&mut self, instance: WorldInstanceId) {
        self.release_world_interests(instance);
    }
}

impl SectorPresentationCache {
    pub(super) fn prepare_scene(
        &mut self,
        cpu: CollisionSnapshot,
        view: minimium_renderer::terrain_view::TerrainView,
        origin: [f32; 3],
        origin_generation: u64,
        geography: minimium_procgen::TerrainInputs,
        water: Option<super::sector_worker::WaterPresentation>,
    ) -> Result<Option<ClientSectorScene>, SectorWorkerError> {
        let owner = cpu.owner();
        if !cpu.matches_terrain(&geography) {
            return Err(SectorWorkerError::InvalidRequest);
        }
        let instance = cpu.instance();
        let manifest = cpu.manifest();
        let position = cpu.position();
        let water = water.map(|incoming| {
            let cached = self
                .water_presentations
                .entry(owner)
                .or_insert_with(|| incoming.clone());
            if !Arc::ptr_eq(&cached.snapshot, &incoming.snapshot)
                || cached.catalog.digest != incoming.catalog.digest
            {
                *cached = incoming;
            }
            cached.set_center(position.map(|value| (f64::from(value) / 4.0).floor() * 4.0));
            cached.clone()
        });
        let terrain_revision = cpu.terrain_revision();
        let material_profile = Some(geography.source().material_profile());
        let resolved_relief_policy = geography.source().relief_policy();
        let sectors = cpu.sectors();
        let mut prepared = None;
        let render_sectors = if manifest.topology() == minimium_world::world::WorldTopology::Planet
        {
            if let Some(result) = self.worker.poll(
                owner,
                instance,
                manifest,
                view,
                origin,
                origin_generation,
                water.clone(),
                sectors,
                geography,
            )? {
                self.presentations.insert(owner, result);
            }
            if let Some(result) = self.presentations.get(&owner).filter(|r| {
                r.matches(
                    instance,
                    manifest,
                    cpu.map_fingerprint(),
                    terrain_revision,
                    view,
                    origin,
                    origin_generation,
                    water.as_ref().map(|w| w.level_revision),
                    water.as_ref().map_or(0, |w| w.fluid_revision),
                    material_profile,
                    resolved_relief_policy,
                )
            }) {
                prepared = Some(result.clone());
                result
                    .patches
                    .iter()
                    .map(|p| (p.key, p.lod, p.geometry.clone()))
                    .collect()
            } else if let Some(previous) = self.presentations.get(&owner).filter(|scene| {
                scene.geometry_matches(
                    instance,
                    manifest,
                    cpu.map_fingerprint(),
                    origin,
                    origin_generation,
                    terrain_revision,
                    material_profile,
                    resolved_relief_policy,
                )
            }) {
                prepared = Some(previous.clone());
                previous
                    .patches
                    .iter()
                    .map(|p| (p.key, p.lod, p.geometry.clone()))
                    .collect()
            } else {
                Vec::new()
            }
        } else {
            sectors.iter().map(|(k, g)| (*k, 0, g.clone())).collect()
        };
        Ok(Some(ClientSectorScene {
            owner,
            prepared,
            origin,
            origin_generation,
            render_sectors,
            cpu,
            generation: manifest.generation().revision(),
            content_revision: manifest.content_revision(),
        }))
    }
}
