use super::*;
use crate::environment::weather::clouds::{density, CloudLobe};
use crate::environment::weather::wind::EARTH_RADIUS;
use std::time::Duration;

impl CloudInstances {
    pub(in crate::renderer) const BYTES: u64 =
        (MAX_INSTANCES * std::mem::size_of::<CloudInstanceGpu>()) as u64
            + std::mem::size_of::<InstanceUniform>() as u64
            + shadow::BYTES
            + super::super::cloud_mesh::CloudMeshes::BYTES;

    pub(in crate::renderer) fn new(device: &wgpu::Device, optical_scale: f32) -> Self {
        let (sender, jobs) = mpsc::sync_channel::<Request>(1);
        let (results, receiver) = mpsc::sync_channel(1);
        std::thread::Builder::new()
            .name("cloud-instances".into())
            .spawn(move || {
                let mut cache = cache::BuildCache::default();
                let mut previous = None;
                while let Ok(mut request) = jobs.recv() {
                    while let Ok(newer) = jobs.try_recv() {
                        request = newer;
                    }
                    if previous
                        .as_ref()
                        .is_some_and(|old: &Request| !old.same_world(&request))
                    {
                        cache = cache::BuildCache::default();
                    }
                    previous = Some(request.clone());
                    if results.send(build::build(request, &mut cache)).is_err() {
                        break;
                    }
                }
            })
            .expect("create cloud field worker");
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("analytic cloud instances"),
            size: ((MAX_INSTANCES * std::mem::size_of::<CloudInstanceGpu>()) as u64)
                .min(device.limits().max_storage_buffer_binding_size)
                .min(device.limits().max_buffer_size),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("cloud representation frame"),
            size: std::mem::size_of::<InstanceUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("cloud instance storage"),
            entries: &[0, 1].map(|binding| wgpu::BindGroupLayoutEntry {
                binding,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: if binding == 0 {
                        wgpu::BufferBindingType::Storage { read_only: true }
                    } else {
                        wgpu::BufferBindingType::Uniform
                    },
                    has_dynamic_offset: false,
                    min_binding_size: wgpu::BufferSize::new(if binding == 0 { 176 } else { 96 }),
                },
                count: None,
            }),
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("cloud instance data"),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: uniform.as_entire_binding(),
                },
            ],
        });
        Self {
            lod_history: LodHistory::new(),
            layout,
            bind_group,
            buffer,
            uniform,
            count: 0,
            draw_count: 0,
            active_limit: 0,
            active_aggregate_limit: 0,
            far: super::super::cloud_mesh::CloudMeshes::new(device),
            shadows: shadow::texture(device),
            morph: 0.0,
            region: [0.0; 4],
            atlas: None,
            sender,
            receiver,
            requested: None,
            published: None,
            frame: None,
            ready: None,
            last_request: Instant::now() - Duration::from_secs(1),
            transition_start: Instant::now(),
            origin: DVec3::ZERO,
            optical_scale,
            local_region: [0.0; 4],
            serial: 0,
            failed: false,
            last_seconds: None,
            build_seconds: 0.5,
            diagnostics: diagnostics::Diagnostics::default(),
        }
    }

    pub(in crate::renderer) fn upload_view(
        &self,
        queue: &wgpu::Queue,
        eye: [f64; 3],
        settings: EffectiveCloudSettings,
        size: [u32; 2],
    ) {
        let view = InstanceUniform {
            eye: (DVec3::from_array(eye) - self.origin)
                .as_vec3()
                .extend(self.optical_scale)
                .to_array(),
            origin: self.origin.as_vec3().extend(0.0).to_array(),
            local_region: if self.draw_count == 0 {
                [0.0; 4]
            } else {
                self.local_region
            },
            config: [
                self.transition_start.elapsed().as_secs_f32().min(0.3) / 0.3,
                settings.bins as f32,
                settings.neighbors as f32,
                settings.debug as u32 as f32,
            ],
            viewport: [
                size[0] as f32,
                size[1] as f32,
                settings.scale,
                self.draw_count as f32,
            ],
            appearance: [
                f32::from(settings.method == super::super::cloud_settings::CloudMethod::Volumetric),
                settings.normal_detail,
                0.0,
                0.0,
            ],
        };
        queue.write_buffer(&self.uniform, 0, bytemuck::bytes_of(&view));
    }
}

pub(super) fn instance(
    pair: &[CloudLobe; 2],
    origin: DVec3,
    radius: f64,
    sign: f32,
) -> CloudInstanceGpu {
    let scale = radius / EARTH_RADIUS;
    let state = |lobe: &CloudLobe| {
        let center = (lobe.center * scale - origin)
            .as_vec3()
            .extend(0.0)
            .to_array();
        let axes = (lobe.axes * scale)
            .as_vec3()
            .extend((lobe.mass * scale * scale) as f32)
            .to_array();
        let base = lobe
            .base_normal
            .as_vec3()
            .extend((lobe.base_offset * scale - origin.dot(lobe.base_normal)) as f32)
            .to_array();
        debug_assert!(density::volume(lobe).is_finite());
        (center, axes, lobe.orientation.as_quat().to_array(), base)
    };
    let (center_a, axes_a, orientation_a, base_a) = state(&pair[0]);
    let (center_b, axes_b, orientation_b, base_b) = state(&pair[1]);
    CloudInstanceGpu {
        center_a,
        axes_a,
        orientation_a,
        base_a,
        center_b,
        axes_b,
        orientation_b,
        base_b,
        neighbors_a: [u32::MAX; 4],
        neighbors_b: [u32::MAX; 4],
        material: [
            f32::from(pair[0].layer),
            (pair[0].id.hash() >> 40) as f32 / 16777216.0,
            (31 - pair[0].id.path.leading_zeros()) as f32,
            sign,
        ],
    }
}
