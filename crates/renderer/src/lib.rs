//! Presentation contracts, deterministic visual preparation and GPU rendering primitives.
//!
//! The client adapter supplies pixel dimensions, prepared scene values and atlas data;
//! this crate owns renderer-specific geometry layouts, resources and pipelines. Window
//! lifecycle, sessions, simulation and frame scheduling remain application concerns.

#[cfg(feature = "gpu")]
pub mod bolid_light;
pub mod atmosphere;
pub mod atmosphere_debug;
#[cfg(feature = "gpu")]
pub mod atmosphere_gpu;
#[cfg(feature = "gpu")]
pub mod atmosphere_pipeline;
#[cfg(feature = "gpu")]
pub mod atmosphere_resources;
#[cfg(feature = "gpu")]
pub mod atmosphere_table;
#[cfg(feature = "gpu")]
mod atmosphere_timing;
#[cfg(feature = "gpu")]
pub mod atmosphere_uniform;
#[cfg(feature = "gpu")]
pub mod atmosphere_view;
#[cfg(feature = "gpu")]
pub mod catalogue;
#[cfg(feature = "gpu")]
pub mod celestial_halo;
#[cfg(feature = "gpu")]
pub mod cloud_face;
#[cfg(feature = "gpu")]
pub mod cloud_gpu;
#[cfg(feature = "gpu")]
pub mod cloud_mesh;
#[cfg(feature = "gpu")]
pub mod cloud_pipeline;
#[cfg(feature = "gpu")]
pub mod cloud_targets;
#[cfg(feature = "gpu")]
pub mod cloud_uniform;
#[cfg(feature = "gpu")]
pub mod cloud_voxels;
#[cfg(feature = "gpu")]
mod coast_probe;
#[cfg(feature = "gpu")]
pub mod environment_pipelines;
#[cfg(feature = "gpu")]
pub mod frame_encoder;
#[cfg(feature = "gpu")]
pub mod geometry;
#[cfg(feature = "gpu")]
pub mod geometry_gpu;
#[cfg(feature = "gpu")]
pub mod gpu_bindings;
#[cfg(feature = "gpu")]
pub mod gpu_frame;
#[cfg(feature = "gpu")]
pub mod lighting;
#[cfg(feature = "gpu")]
pub mod milky_way_gpu;
mod bolid_dust;
mod bolid_layer;
#[cfg(feature = "gpu")]
pub mod moon_disk;
#[cfg(feature = "gpu")]
pub mod moon_lighting;
#[cfg(feature = "gpu")]
pub mod moon_sphere;
#[cfg(feature = "gpu")]
pub mod ocean_geometry;
#[cfg(feature = "gpu")]
mod ocean_pipeline;
#[cfg(feature = "gpu")]
pub mod output;
#[cfg(feature = "gpu")]
pub mod overview_gpu;
#[cfg(feature = "gpu")]
pub mod planet_visibility;
#[cfg(feature = "gpu")]
pub mod prepared_sector;
#[cfg(feature = "gpu")]
pub mod profiles;
pub mod quality;
#[cfg(feature = "gpu")]
pub mod render_frame;
#[cfg(feature = "gpu")]
pub mod render_patch_cache;
#[cfg(feature = "gpu")]
pub mod scene_encoder;
#[cfg(feature = "gpu")]
pub mod shader_source;
#[cfg(feature = "gpu")]
pub mod shadow_geometry;
#[cfg(feature = "gpu")]
pub mod shadow_gpu;
#[cfg(feature = "gpu")]
pub mod shadow_settings;
#[cfg(feature = "gpu")]
pub mod shadow_target;
#[cfg(feature = "gpu")]
pub mod shadow_uniform;
#[cfg(feature = "gpu")]
pub mod star_gpu;
#[cfg(feature = "gpu")]
pub mod stars;
#[cfg(feature = "gpu")]
pub mod surface_atlas;
#[cfg(feature = "gpu")]
pub mod surface_cover;
#[cfg(feature = "gpu")]
pub mod surface_frame;
#[cfg(feature = "gpu")]
mod terrain_cover_gpu;
pub mod terrain_debug;
#[cfg(feature = "gpu")]
pub mod terrain_edges;
#[cfg(feature = "gpu")]
pub mod terrain_geometry;
#[cfg(feature = "gpu")]
pub mod terrain_gpu;
#[cfg(feature = "gpu")]
mod terrain_palette;
#[cfg(feature = "gpu")]
pub mod terrain_view;
#[cfg(feature = "gpu")]
pub mod ui_pipeline;
#[cfg(feature = "gpu")]
pub mod ui_vertex;
#[cfg(feature = "gpu")]
pub mod world_geometry_gpu;
#[cfg(feature = "gpu")]
pub mod world_pipeline;

/// Compile-time renderer shader sources.
pub mod shaders {
    /// Shadow-map pass source.
    pub const SHADOW_WGSL: &str = include_str!("shaders/shadow.wgsl");
}

#[cfg(feature = "gpu")]
pub use atmosphere_gpu::AtmosphereGpu;
#[cfg(feature = "gpu")]
pub use atmosphere_uniform::AtmosphereUniform;
#[cfg(feature = "gpu")]
pub use celestial_halo::CelestialHaloGpu;
#[cfg(feature = "gpu")]
pub use cloud_face::CloudFace;
#[cfg(feature = "gpu")]
pub use cloud_gpu::CloudGpu;
#[cfg(feature = "gpu")]
pub use cloud_targets::CloudTargets;
#[cfg(feature = "gpu")]
pub use cloud_uniform::{CloudRecord, CloudUniform};
#[cfg(feature = "gpu")]
pub use environment_pipelines::EnvironmentPipelines;
#[cfg(feature = "gpu")]
pub use milky_way_gpu::MilkyWayGpu;
#[cfg(feature = "gpu")]
pub use moon_disk::MoonGpu;
#[cfg(feature = "gpu")]
pub use moon_sphere::MoonSphereGpu;
#[cfg(feature = "gpu")]
pub use output::OutputEncoder;
#[cfg(feature = "gpu")]
pub use render_frame::RenderFrame;
#[cfg(feature = "gpu")]
pub use shadow_gpu::ShadowGpu;
#[cfg(feature = "gpu")]
pub use shadow_settings::ShadowSettings;
#[cfg(feature = "gpu")]
pub use star_gpu::StarGpu;
#[cfg(feature = "gpu")]
pub use ui_vertex::UiVertex;

/// Selects the camera projection context used by world rendering.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum EnvironmentViewKind {
    /// Planet surface view.
    Surface = 0,
    /// Planet overview view.
    Overview = 1,
}

/// Camera and environment values prepared for one rendered frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RenderViewFrame {
    pub view: [[f32; 4]; 4],
    pub projection: [[f32; 4]; 4],
    pub view_projection: [[f32; 4]; 4],
    pub sky_view_projection: [[f32; 4]; 4],
    pub inverse_sky_view_projection: [[f32; 4]; 4],
    pub viewport: [u32; 2],
    pub render_origin_m: [f64; 3],
    pub camera_eye_world_m: [f64; 3],
    pub game_planet_radius_m: f64,
    pub kind: EnvironmentViewKind,
    pub srgb_surface: bool,
}

/// Camera input supplied by the client presentation adapter.
pub struct RenderViewInput {
    pub view: glam::Mat4,
    pub projection: glam::Mat4,
    pub viewport: [u32; 2],
    pub render_origin_m: [f64; 3],
    pub camera_eye_world_m: [f64; 3],
    pub game_planet_radius_m: f64,
    pub kind: EnvironmentViewKind,
    pub srgb_surface: bool,
}

impl Default for RenderViewFrame {
    fn default() -> Self {
        let identity = [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ];
        Self {
            view: identity,
            projection: identity,
            view_projection: identity,
            sky_view_projection: identity,
            inverse_sky_view_projection: identity,
            viewport: [1, 1],
            render_origin_m: [0.0; 3],
            camera_eye_world_m: [0.0, 1.0, 0.0],
            game_planet_radius_m: 0.0,
            kind: EnvironmentViewKind::Surface,
            srgb_surface: true,
        }
    }
}

impl RenderViewFrame {
    /// Builds the finite camera matrices and frame metadata from application input.
    pub fn from_camera(input: RenderViewInput) -> Option<Self> {
        let RenderViewInput {
            view,
            projection,
            viewport,
            render_origin_m,
            camera_eye_world_m,
            game_planet_radius_m,
            kind,
            srgb_surface,
        } = input;
        let mut sky_view = view;
        sky_view.w_axis = glam::Vec4::W;
        let sky_view_projection = projection * sky_view;
        let inverse_sky_view_projection = sky_view_projection.inverse();
        let view_projection = projection * view;
        let frame = Self {
            view: view.to_cols_array_2d(),
            projection: projection.to_cols_array_2d(),
            view_projection: view_projection.to_cols_array_2d(),
            sky_view_projection: sky_view_projection.to_cols_array_2d(),
            inverse_sky_view_projection: inverse_sky_view_projection.to_cols_array_2d(),
            viewport: [viewport[0].max(1), viewport[1].max(1)],
            render_origin_m,
            camera_eye_world_m,
            game_planet_radius_m,
            kind,
            srgb_surface,
        };
        frame.is_finite().then_some(frame)
    }

    /// Returns whether all frame values satisfy the renderer contract.
    pub fn is_finite(self) -> bool {
        self.view_projection
            .iter()
            .chain(self.view.iter())
            .chain(self.projection.iter())
            .chain(self.sky_view_projection.iter())
            .chain(self.inverse_sky_view_projection.iter())
            .flatten()
            .all(|value| value.is_finite())
            && self.render_origin_m.iter().all(|value| value.is_finite())
            && self
                .camera_eye_world_m
                .iter()
                .all(|value| value.is_finite())
            && self.game_planet_radius_m.is_finite()
            && self.game_planet_radius_m >= 0.0
    }
}

#[cfg(feature = "gpu")]
mod geometry_rebase;

#[cfg(feature = "gpu")]
pub mod shadow_view;
