use super::{pipeline, targets, Face, MAX_FACES};
use crate::environment::weather::MAX_CLOUDS;
use bytemuck::{Pod, Zeroable};

#[repr(C, align(16))]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(super) struct CloudUniform {
    pub projection: [[f32; 4]; 4],
    pub scene_projection: [[f32; 4]; 4],
    pub inverse_projection: [[f32; 4]; 4],
    pub origin_radius: [f32; 4],
    pub eye: [f32; 4],
    pub observer: [f32; 4],
    pub config: [u32; 4],
}

#[repr(C, align(16))]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(super) struct CloudRecord {
    pub center_age: [f32; 4],
    pub east_opacity: [f32; 4],
    pub up_layer: [f32; 4],
    pub north_region: [f32; 4],
    pub morph: [f32; 4],
}

const _: () = {
    assert!(std::mem::size_of::<CloudUniform>() == 256);
    assert!(std::mem::size_of::<CloudRecord>() == 80);
    assert!(std::mem::size_of::<Face>() == 48);
    assert!(std::mem::align_of::<CloudUniform>() == 16);
    assert!(std::mem::offset_of!(CloudUniform, origin_radius) == 192);
    assert!(std::mem::offset_of!(CloudRecord, north_region) == 48);
    assert!(std::mem::offset_of!(CloudRecord, morph) == 64);
};

pub(crate) struct CloudGpu {
    pub(super) pipeline: wgpu::RenderPipeline,
    pub(super) debug_pipeline: wgpu::RenderPipeline,
    pub(super) composite_pipeline: Option<wgpu::RenderPipeline>,
    pub(super) group: wgpu::BindGroup,
    pub(super) targets: Option<targets::Targets>,
    pub(super) uniform: wgpu::Buffer,
    pub(super) records: wgpu::Buffer,
    pub(super) faces: wgpu::Buffer,
    pub(super) count: u32,
    pub(super) revision: u64,
    pub(super) debug: bool,
    pub(super) oit: bool,
    layout: wgpu::BindGroupLayout,
    composite_layout: wgpu::BindGroupLayout,
}

impl CloudGpu {
    pub(crate) const BUFFER_BYTES: u64 = (MAX_FACES * 48 + MAX_CLOUDS * 80 + 256) as u64;

    pub(crate) fn new(
        device: &wgpu::Device,
        adapter: &wgpu::Adapter,
        frame: &wgpu::BindGroupLayout,
        format: wgpu::TextureFormat,
        depth: &wgpu::TextureView,
        size: winit::dpi::PhysicalSize<u32>,
    ) -> Self {
        let oit = [
            wgpu::TextureFormat::Rgba16Float,
            wgpu::TextureFormat::R16Float,
        ]
        .iter()
        .all(|format| {
            let f = adapter.get_texture_format_features(*format);
            f.allowed_usages.contains(
                wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            ) && f.flags.contains(wgpu::TextureFormatFeatureFlags::BLENDABLE)
        }) && device.limits().max_color_attachment_bytes_per_sample >= 10;
        eprintln!(
            "weather_transparency={} buffer_limit_bytes={} target_limit_bytes={}",
            if oit {
                "weighted_blended_oit"
            } else {
                "sorted_face_alpha"
            },
            Self::BUFFER_BYTES,
            targets::MAX_TARGET_BYTES
        );
        let buffer = |label, size, usage| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size,
                usage: usage | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        };
        let uniform = buffer("cloud uniform", 256, wgpu::BufferUsages::UNIFORM);
        let records = buffer(
            "cloud transforms",
            (MAX_CLOUDS * 80) as u64,
            wgpu::BufferUsages::STORAGE,
        );
        let faces = buffer(
            "bounded greedy cloud faces",
            (MAX_FACES * 48) as u64,
            wgpu::BufferUsages::VERTEX,
        );
        let layout = pipeline::layout(device);
        let composite_layout = targets::layout(device);
        let group = pipeline::group(device, &layout, &uniform, &records, depth);
        let pipeline = pipeline::create(device, frame, &layout, format, oit, false);
        let debug_pipeline = pipeline::create(device, frame, &layout, format, false, true);
        let composite_pipeline =
            oit.then(|| pipeline::composite(device, &composite_layout, format));
        let targets = oit.then(|| targets::Targets::new(device, &composite_layout, size));
        Self {
            pipeline,
            debug_pipeline,
            composite_pipeline,
            group,
            targets,
            uniform,
            records,
            faces,
            count: 0,
            revision: 0,
            debug: false,
            oit,
            layout,
            composite_layout,
        }
    }

    pub(crate) fn resize(
        &mut self,
        device: &wgpu::Device,
        depth: &wgpu::TextureView,
        size: winit::dpi::PhysicalSize<u32>,
    ) {
        self.group = pipeline::group(device, &self.layout, &self.uniform, &self.records, depth);
        self.targets = self
            .oit
            .then(|| targets::Targets::new(device, &self.composite_layout, size));
    }

    pub(crate) fn bytes(&self) -> u64 {
        Self::BUFFER_BYTES + self.targets.as_ref().map_or(0, |t| t.bytes)
    }
}
