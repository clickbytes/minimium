use crate::SessionFailure;
use minimium_protocol::WorldContext;
use minimium_world::world::{TransitionId, WorldManifest, WorldMap};
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
    pub ticket: ContentResolutionTicket,
    pub manifest: WorldManifest,
    pub map: Arc<WorldMap>,
    pub geography: Option<Arc<dyn minimium_procgen::PreparedGeography>>,
    pub terrain_material_profile: minimium_procgen::ResolvedTerrainMaterialProfile,
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
    pub fn matches(&self, ticket: ContentResolutionTicket, manifest: WorldManifest) -> bool {
        self.ticket == ticket
            && self.manifest == manifest
            && self
                .terrain_material_profile
                .matches(manifest, self.map.fingerprint())
    }
}
