use super::environment::{
    EnvironmentFrame, EnvironmentViewKind, HaloSource, RenderViewFrame, MAX_HALO_BODIES,
};
use bytemuck::{Pod, Zeroable};
use std::sync::OnceLock;
use std::time::Instant;
use wgpu::util::DeviceExt;

#[repr(C, align(16))]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable)]
pub(super) struct CameraUniform {
    pub(super) view_projection: [[f32; 4]; 4],
    pub(super) sky_view_projection: [[f32; 4]; 4],
    pub(super) inverse_sky_view_projection: [[f32; 4]; 4],
    pub(super) viewport: [f32; 4],
    pub(super) eye_world: [f32; 4],
}

#[repr(C, align(16))]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable)]
pub(super) struct EnvironmentUniform {
    pub(super) sun_geometric_world: [f32; 4],
    pub(super) sun_apparent_world: [f32; 4],
    pub(super) moon_geometric_world: [f32; 4],
    pub(super) moon_apparent_world: [f32; 4],
    pub(super) moon_light_world_phase: [f32; 4],
    pub(super) moon_pole_world: [f32; 4],
    pub(super) local_sun_rgb_visibility: [f32; 4],
    pub(super) local_moon_rgb_visibility: [f32; 4],
    pub(super) global_sun_direction: [f32; 4],
    pub(super) global_sun_rgb: [f32; 4],
    pub(super) ambient_rgb_exposure: [f32; 4],
    pub(super) sky_zenith_rgb_sun_altitude: [f32; 4],
    pub(super) sky_horizon_rgb_star_visibility: [f32; 4],
    pub(super) observer_up_horizon_cos: [f32; 4],
    pub(super) flags: [u32; 4],
    pub(super) eqj_to_world: [[f32; 4]; 4],
    pub(super) moon_disk_radiance: [f32; 4],
    pub(super) star_animation: [f32; 4],
    pub(super) moon_orbit_world: [f32; 4],
    pub(super) halo_count: [u32; 4],
    pub(super) halo_sources: [HaloSource; MAX_HALO_BODIES],
}

pub(super) struct EnvironmentGpuBindings {
    pub(super) layout: wgpu::BindGroupLayout,
    pub(super) camera_buffer: wgpu::Buffer,
    pub(super) environment_buffer: wgpu::Buffer,
    pub(super) bind_group: wgpu::BindGroup,
}

impl EnvironmentGpuBindings {
    pub(super) fn create(device: &wgpu::Device) -> Self {
        let camera_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("camera-uniform"),
            contents: bytemuck::bytes_of(&CameraUniform::zeroed()),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let environment_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("environment-uniform"),
            contents: bytemuck::bytes_of(&EnvironmentUniform::zeroed()),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("world-frame-layout"),
            entries: &[
                uniform_entry(0, std::mem::size_of::<CameraUniform>() as u64),
                uniform_entry(1, std::mem::size_of::<EnvironmentUniform>() as u64),
            ],
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("world-frame-bind-group"),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: camera_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: environment_buffer.as_entire_binding(),
                },
            ],
        });
        Self {
            layout,
            camera_buffer,
            environment_buffer,
            bind_group,
        }
    }
}

pub(super) fn camera_uniform(view: RenderViewFrame) -> CameraUniform {
    CameraUniform {
        view_projection: view.view_projection,
        sky_view_projection: view.sky_view_projection,
        inverse_sky_view_projection: view.inverse_sky_view_projection,
        viewport: [
            view.viewport[0] as f32,
            view.viewport[1] as f32,
            1.0 / view.viewport[0].max(1) as f32,
            1.0 / view.viewport[1].max(1) as f32,
        ],
        eye_world: [
            view.camera_eye_world_m[0] as f32,
            view.camera_eye_world_m[1] as f32,
            view.camera_eye_world_m[2] as f32,
            0.0,
        ],
    }
}

pub(super) fn environment_uniform(frame: &EnvironmentFrame) -> EnvironmentUniform {
    static ANIMATION_EPOCH: OnceLock<Instant> = OnceLock::new();
    let mut uniform = frame.environment_uniform;
    uniform.star_animation[0] = ANIMATION_EPOCH
        .get_or_init(Instant::now)
        .elapsed()
        .as_secs_f32();
    uniform.flags[1] = match frame.render_view.kind {
        EnvironmentViewKind::Surface => 0,
        EnvironmentViewKind::Overview => 1,
    };
    uniform.flags[2] =
        u32::from(frame.status != super::environment::EnvironmentFrameStatus::Unavailable);
    uniform.flags[3] = u32::from(!frame.render_view.srgb_surface);
    uniform.halo_count = [frame.halo_count.min(MAX_HALO_BODIES as u32), 0, 0, 0];
    uniform.halo_sources = frame.halo_sources;
    uniform
}

fn uniform_entry(binding: u32, size: u64) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: wgpu::BufferSize::new(size),
        },
        count: None,
    }
}

const _: () = {
    assert!(std::mem::size_of::<CameraUniform>() == 224);
    assert!(std::mem::align_of::<CameraUniform>() == 16);
    assert!(std::mem::offset_of!(CameraUniform, view_projection) == 0);
    assert!(std::mem::offset_of!(CameraUniform, sky_view_projection) == 64);
    assert!(std::mem::offset_of!(CameraUniform, inverse_sky_view_projection) == 128);
    assert!(std::mem::offset_of!(CameraUniform, viewport) == 192);
    assert!(std::mem::offset_of!(CameraUniform, eye_world) == 208);

    assert!(std::mem::size_of::<HaloSource>() == 64);
    assert!(std::mem::align_of::<HaloSource>() == 16);
    assert!(std::mem::size_of::<EnvironmentUniform>() == 880);
    assert!(std::mem::align_of::<EnvironmentUniform>() == 16);
    assert!(std::mem::offset_of!(EnvironmentUniform, sun_geometric_world) == 0);
    assert!(std::mem::offset_of!(EnvironmentUniform, sun_apparent_world) == 16);
    assert!(std::mem::offset_of!(EnvironmentUniform, moon_geometric_world) == 32);
    assert!(std::mem::offset_of!(EnvironmentUniform, moon_apparent_world) == 48);
    assert!(std::mem::offset_of!(EnvironmentUniform, moon_light_world_phase) == 64);
    assert!(std::mem::offset_of!(EnvironmentUniform, moon_pole_world) == 80);
    assert!(std::mem::offset_of!(EnvironmentUniform, local_sun_rgb_visibility) == 96);
    assert!(std::mem::offset_of!(EnvironmentUniform, local_moon_rgb_visibility) == 112);
    assert!(std::mem::offset_of!(EnvironmentUniform, global_sun_direction) == 128);
    assert!(std::mem::offset_of!(EnvironmentUniform, global_sun_rgb) == 144);
    assert!(std::mem::offset_of!(EnvironmentUniform, ambient_rgb_exposure) == 160);
    assert!(std::mem::offset_of!(EnvironmentUniform, sky_zenith_rgb_sun_altitude) == 176);
    assert!(std::mem::offset_of!(EnvironmentUniform, sky_horizon_rgb_star_visibility) == 192);
    assert!(std::mem::offset_of!(EnvironmentUniform, observer_up_horizon_cos) == 208);
    assert!(std::mem::offset_of!(EnvironmentUniform, flags) == 224);
    assert!(std::mem::offset_of!(EnvironmentUniform, eqj_to_world) == 240);
    assert!(std::mem::offset_of!(EnvironmentUniform, moon_disk_radiance) == 304);
    assert!(std::mem::offset_of!(EnvironmentUniform, star_animation) == 320);
    assert!(std::mem::offset_of!(EnvironmentUniform, moon_orbit_world) == 336);
    assert!(std::mem::offset_of!(EnvironmentUniform, halo_count) == 352);
    assert!(std::mem::offset_of!(EnvironmentUniform, halo_sources) == 368);
};
