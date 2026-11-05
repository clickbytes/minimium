//! Bounded cube meshes, worker publication and GPU transparency.
mod cache;
mod runtime;
mod worker;

pub(super) use minimium_renderer::CloudGpu;
pub(super) use runtime::WeatherState;

use glam::DVec3;
use minimium_environment::weather::CloudSnapshot;
use minimium_protocol::WorldContext;
use minimium_world::world::WorldManifest;
use std::{sync::Arc, time::Instant};

pub(super) const MAX_FACES: usize = minimium_renderer::cloud_face::MAX_CLOUD_FACES;

pub(super) use minimium_renderer::CloudFace as Face;

#[derive(Clone, Copy, PartialEq, Eq)]
struct Stamp {
    manifest: WorldManifest,
    context: WorldContext,
    clock_revision: u64,
}

#[derive(Clone)]
struct Request {
    stamp: Stamp,
    utc: f64,
    observer: DVec3,
    eye: DVec3,
    sort: bool,
    revision: u64,
    detail: u8,
    started: Instant,
    geography: Option<Arc<dyn minimium_procgen::PreparedGeography>>,
}

struct Publication {
    request: Request,
    snapshot: CloudSnapshot,
    faces: Vec<Face>,
    morphs: Vec<cache::Morph>,
    stats: cache::MeshStats,
    limited: bool,
    build_ms: f64,
}
