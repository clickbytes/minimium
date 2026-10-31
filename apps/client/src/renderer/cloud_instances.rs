//! GPU representations and bounded asynchronous publication of physical clouds.
mod build;
mod cache;
mod diagnostics;
mod gpu;
mod lod;
mod publication;
mod runtime;
pub(super) mod shadow;

use super::cloud_settings::EffectiveCloudSettings;
use crate::environment::weather::clouds::{atmosphere::Atlas, runtime::PresentedCloudFrame};
use bytemuck::{Pod, Zeroable};
use glam::DVec3;
use minimium_core::world::WorldInstanceId;
use std::{
    collections::HashMap,
    sync::{mpsc, Arc},
    time::Instant,
};

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(super) struct CloudInstanceGpu {
    center_a: [f32; 4],
    axes_a: [f32; 4],
    orientation_a: [f32; 4],
    base_a: [f32; 4],
    center_b: [f32; 4],
    axes_b: [f32; 4],
    orientation_b: [f32; 4],
    base_b: [f32; 4],
    neighbors_a: [u32; 4],
    neighbors_b: [u32; 4],
    material: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct InstanceUniform {
    eye: [f32; 4],
    origin: [f32; 4],
    local_region: [f32; 4],
    config: [f32; 4],
    viewport: [f32; 4],
    appearance: [f32; 4],
}

const _: () = {
    assert!(std::mem::size_of::<CloudInstanceGpu>() == 176);
    assert!(std::mem::offset_of!(CloudInstanceGpu, center_b) == 64);
    assert!(std::mem::offset_of!(CloudInstanceGpu, neighbors_a) == 128);
    assert!(std::mem::offset_of!(CloudInstanceGpu, material) == 160);
    assert!(std::mem::size_of::<InstanceUniform>() == 96);
};

#[derive(Clone)]
struct Request {
    world: WorldInstanceId,
    previous_lod: LodHistory,
    atlas: Arc<Atlas>,
    eye: DVec3,
    radius: f64,
    pixels_per_radian: f64,
    start_seconds: f64,
    duration: f64,
    clock_revision: Option<u64>,
    blend: f32,
    seed: u128,
    settings: EffectiveCloudSettings,
    serial: u64,
}

impl Request {
    fn blend_at(&self, seconds: f64) -> f32 {
        self.atlas.start_seconds.map_or(self.blend, |start| {
            ((seconds - start) / 600.0).clamp(0.0, 1.0) as f32
        })
    }
    fn same_world(&self, other: &Self) -> bool {
        self.world == other.world && self.seed == other.seed
            && self.clock_revision == other.clock_revision
            && self.radius == other.radius
    }
    fn needs_update(&self, other: &Self) -> bool {
        !self.same_world(other)
            || self.atlas.revision != other.atlas.revision
            || self.start_seconds != other.start_seconds
            || self.duration != other.duration
            || !self.settings.same_representation(other.settings)
            || (self.pixels_per_radian / other.pixels_per_radian - 1.0).abs() > 0.1
            || self.eye.distance(other.eye) > self.radius * 0.0001
    }
}

pub(super) struct View {
    pub(super) world: WorldInstanceId,
    pub(super) eye: [f64; 3],
    pub(super) radius: f64,
    pub(super) pixels_per_radian: f64,
    pub(super) utc_ms: i64,
    pub(super) clock_revision: Option<u64>,
    pub(super) blend: f32,
    pub(super) seed: u128,
    pub(super) world_rate: f64,
    pub(super) settings: EffectiveCloudSettings,
}

pub(super) struct CloudInstances {
    lod_history: LodHistory,
    pub(super) layout: wgpu::BindGroupLayout,
    pub(super) bind_group: wgpu::BindGroup,
    buffer: wgpu::Buffer,
    uniform: wgpu::Buffer,
    pub(super) count: u32,
    pub(super) draw_count: u32,
    active_limit: u32,
    active_aggregate_limit: usize,
    pub(super) far: super::cloud_mesh::CloudMeshes,
    pub(super) shadows: wgpu::Texture,
    pub(super) morph: f32,
    pub(super) region: [f32; 4],
    atlas: Option<Arc<Atlas>>,
    sender: mpsc::SyncSender<Request>,
    receiver: mpsc::Receiver<build::Built>,
    requested: Option<Request>,
    published: Option<Request>,
    frame: Option<PresentedCloudFrame>,
    ready: Option<build::Built>,
    last_request: Instant,
    transition_start: Instant,
    origin: DVec3,
    optical_scale: f32,
    local_region: [f32; 4],
    serial: u64,
    failed: bool,
    last_seconds: Option<f64>,
    build_seconds: f64,
    diagnostics: diagnostics::Diagnostics,
}

pub(super) const MAX_INSTANCES: usize = 32768;
type LodHistory = HashMap<crate::environment::weather::clouds::CloudId, u8>;
