//! Bounded cube meshes, worker publication and GPU transparency.
mod cache;
mod gpu;
mod mesh;
mod pipeline;
mod render;
mod runtime;
mod targets;
mod voxels;
mod worker;

pub(super) use gpu::CloudGpu;
pub(super) use runtime::WeatherState;

use crate::environment::weather::CloudSnapshot;
use bytemuck::{Pod, Zeroable};
use glam::DVec3;
use minimium_core::world::WorldManifest;
use minimium_protocol::WorldContext;
use std::time::Instant;

pub(super) const MAX_FACES: usize = 131_072;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Face {
    center: [f32; 4],
    u: [f32; 4],
    v: [f32; 4],
}

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
