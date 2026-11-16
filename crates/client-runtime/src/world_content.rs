use crate::SessionFailure;
use minimium_protocol::WorldContext;
use minimium_world::world::{TransitionId, WorldManifest, WorldMap, WorldSource, WorldTopology};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

#[derive(Clone, Default)]
pub struct ContentCancellation(Arc<AtomicBool>);

impl ContentCancellation {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

#[derive(Clone, Copy)]
pub struct WorldContentRequest {
    pub ticket: ContentResolutionTicket,
    pub manifest: WorldManifest,
    pub expected_fingerprint: u64,
}

pub struct ResolvedWorldContent {
    ticket: ContentResolutionTicket,
    manifest: WorldManifest,
    map: Arc<WorldMap>,
    prepared_terrain: minimium_procgen::PreparedTerrainSource,
}

pub trait WorldContentResolver: Send + Sync {
    fn resolve(
        &self,
        request: WorldContentRequest,
        cancellation: ContentCancellation,
    ) -> Result<ResolvedWorldContent, SessionFailure>;
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ContentResolutionTicket {
    pub session_id: u64,
    pub request_id: u64,
    pub context: WorldContext,
    pub transition_id: Option<TransitionId>,
    pub content_revision: u64,
}

impl ResolvedWorldContent {
    pub const fn ticket(&self) -> ContentResolutionTicket {
        self.ticket
    }
    pub const fn manifest(&self) -> WorldManifest {
        self.manifest
    }
    pub fn map(&self) -> &Arc<WorldMap> {
        &self.map
    }
    pub fn geography(&self) -> Option<&Arc<dyn minimium_procgen::PreparedGeography>> {
        self.prepared_terrain.geography()
    }
    pub const fn terrain_material_profile(
        &self,
    ) -> minimium_procgen::ResolvedTerrainMaterialProfile {
        self.prepared_terrain.material_profile()
    }
    pub const fn resolved_relief_policy(
        &self,
    ) -> Option<minimium_procgen::ResolvedPlanetReliefPolicy> {
        self.prepared_terrain.relief_policy()
    }
    pub fn prepare(
        ticket: ContentResolutionTicket,
        manifest: WorldManifest,
        map: Arc<WorldMap>,
        geography: Option<Arc<dyn minimium_procgen::PreparedGeography>>,
        terrain_material_profile: minimium_procgen::ResolvedTerrainMaterialProfile,
        resolved_relief_policy: Option<minimium_procgen::ResolvedPlanetReliefPolicy>,
    ) -> Result<Self, minimium_procgen::GenerationError> {
        let prepared_terrain = minimium_procgen::PreparedTerrainSource::new(
            ticket.context.instance(),
            manifest,
            &map,
            geography.clone(),
            terrain_material_profile,
            resolved_relief_policy,
        )?;
        Ok(Self {
            ticket,
            manifest,
            map,
            prepared_terrain,
        })
    }

    pub fn prepared_terrain(&self) -> &minimium_procgen::PreparedTerrainSource {
        &self.prepared_terrain
    }

    pub fn matches(&self, ticket: ContentResolutionTicket, manifest: WorldManifest) -> bool {
        let relief_matches = match self.prepared_terrain.relief_policy() {
            Some(policy) => policy.matches(manifest, self.map.fingerprint()),
            None => {
                manifest.source() != WorldSource::Generated
                    || manifest.topology() != WorldTopology::Planet
            }
        };
        self.ticket == ticket
            && self.manifest == manifest
            && self.prepared_terrain.world() == ticket.context.instance()
            && self
                .prepared_terrain
                .matches_map(ticket.context.instance(), manifest, &self.map)
            && relief_matches
            && self
                .prepared_terrain
                .material_profile()
                .matches(manifest, self.map.fingerprint())
    }
}
