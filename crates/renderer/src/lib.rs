//! Presentation contracts, deterministic visual preparation and GPU rendering primitives.
//!
//! The client adapter supplies pixel dimensions, prepared scene values and atlas data;
//! this crate owns renderer-specific geometry layouts, resources and pipelines. Window
//! lifecycle, sessions, simulation and frame scheduling remain application concerns.

pub mod atmosphere;
pub mod atmosphere_gpu;
pub mod atmosphere_pipeline;
pub mod atmosphere_table;
pub mod atmosphere_uniform;
pub mod catalogue;
pub mod celestial_halo;
pub mod cloud_face;
pub mod cloud_gpu;
pub mod cloud_mesh;
pub mod cloud_pipeline;
pub mod cloud_targets;
pub mod cloud_uniform;
pub mod cloud_voxels;
pub mod environment_pipelines;
pub mod frame_encoder;
pub mod geometry;
pub mod geometry_gpu;
pub mod globe_detail;
pub mod globe_gpu;
pub mod globe_vertex;
pub mod gpu_bindings;
pub mod gpu_frame;
pub mod lighting;
pub mod milky_way_gpu;
pub mod moon_disk;
pub mod moon_lighting;
pub mod moon_sphere;
pub mod output;
pub mod overview_gpu;
pub mod planet_visibility;
pub mod profiles;
pub mod quality;
pub mod render_frame;
pub mod scene_encoder;
pub mod shader_source;
pub mod shadow_geometry;
pub mod shadow_gpu;
pub mod shadow_settings;
pub mod shadow_target;
pub mod shadow_uniform;
pub mod star_gpu;
pub mod stars;
pub mod surface_frame;
pub mod ui_pipeline;
pub mod ui_vertex;
pub mod world_geometry_gpu;
pub mod world_pipeline;

/// Compile-time renderer shader sources.
pub mod shaders {
    /// Shadow-map pass source.
    pub const SHADOW_WGSL: &str = include_str!("shaders/shadow.wgsl");
}

pub use atmosphere_gpu::AtmosphereGpu;
pub use atmosphere_uniform::AtmosphereUniform;
pub use celestial_halo::CelestialHaloGpu;
pub use cloud_face::CloudFace;
pub use cloud_gpu::CloudGpu;
pub use cloud_targets::CloudTargets;
pub use cloud_uniform::{CloudRecord, CloudUniform};
pub use environment_pipelines::EnvironmentPipelines;
pub use globe_detail::DetailGpu;
pub use globe_gpu::GlobeGpu;
pub use globe_vertex::GlobeVertex;
pub use milky_way_gpu::MilkyWayGpu;
pub use moon_disk::MoonGpu;
pub use moon_sphere::MoonSphereGpu;
pub use output::OutputEncoder;
pub use render_frame::RenderFrame;
pub use shadow_gpu::ShadowGpu;
pub use shadow_settings::ShadowSettings;
pub use star_gpu::StarGpu;
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
