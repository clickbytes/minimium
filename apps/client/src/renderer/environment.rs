mod adapter;
mod clock;
pub mod observer;
mod runtime;
#[allow(dead_code)]
pub mod stars;

use super::environment_gpu::{CameraUniform, EnvironmentUniform};
use minimium_astronomy::{CelestialBodyState, SkyState};
use minimium_core::{environment::EnvironmentDescriptor, sky::ObserverFrame, time::ClockRevision};
use minimium_protocol::WorldContext;

pub(crate) use adapter::EnvironmentFrameError;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub(crate) enum EnvironmentViewKind {
    Surface = 0,
    Overview = 1,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct RenderViewFrame {
    pub(crate) view: [[f32; 4]; 4],
    pub(crate) projection: [[f32; 4]; 4],
    pub(crate) view_projection: [[f32; 4]; 4],
    pub(crate) sky_view_projection: [[f32; 4]; 4],
    pub(crate) inverse_sky_view_projection: [[f32; 4]; 4],
    pub(crate) viewport: [u32; 2],
    pub(crate) render_origin_m: [f64; 3],
    pub(crate) camera_eye_world_m: [f64; 3],
    pub(crate) game_planet_radius_m: f64,
    pub(crate) kind: EnvironmentViewKind,
    pub(crate) srgb_surface: bool,
}

pub(crate) struct RenderViewInput {
    pub(crate) view: glam::Mat4,
    pub(crate) projection: glam::Mat4,
    pub(crate) viewport: [u32; 2],
    pub(crate) render_origin_m: [f64; 3],
    pub(crate) camera_eye_world_m: [f64; 3],
    pub(crate) game_planet_radius_m: f64,
    pub(crate) kind: EnvironmentViewKind,
    pub(crate) srgb_surface: bool,
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
    pub(crate) fn from_camera(input: RenderViewInput) -> Option<Self> {
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

    fn is_finite(self) -> bool {
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

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct EnvironmentSettings {
    /// Linear RGB values in the renderer's relative, unit-sun scale.
    pub(crate) global_sun_rgb: [f32; 3],
    pub(crate) global_sun_strength: f32,
    pub(crate) local_sun_rgb: [f32; 3],
    pub(crate) local_moon_rgb: [f32; 3],
    pub(crate) moon_full_phase_strength: f32,
    pub(crate) ambient_rgb: [f32; 3],
    pub(crate) exposure: f32,
}

impl EnvironmentSettings {
    pub(crate) const fn planetary() -> Self {
        Self {
            global_sun_rgb: [1.0, 0.97, 0.90],
            global_sun_strength: 1.0,
            local_sun_rgb: [1.0, 0.97, 0.90],
            local_moon_rgb: [0.25, 0.32, 0.50],
            moon_full_phase_strength: 0.12,
            ambient_rgb: [1.0, 1.0, 1.0],
            exposure: 1.0,
        }
    }

    pub(crate) const fn interior() -> Self {
        Self {
            global_sun_rgb: [0.0; 3],
            global_sun_strength: 0.0,
            local_sun_rgb: [0.0; 3],
            local_moon_rgb: [0.0; 3],
            moon_full_phase_strength: 0.0,
            ambient_rgb: [0.32, 0.32, 0.32],
            exposure: 1.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct GlobalSunFrame {
    pub(crate) direction_world: [f64; 3],
    pub(crate) linear_rgb: [f32; 3],
    pub(crate) strength: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct LocalBodyFrame {
    pub(crate) geometric_direction_world: [f64; 3],
    pub(crate) apparent_direction_world: [f64; 3],
    pub(crate) geometric_altitude_rad: f64,
    pub(crate) apparent_altitude_rad: f64,
    pub(crate) angular_radius_rad: f64,
    pub(crate) distance_m: f64,
    pub(crate) visible_fraction: f32,
    pub(crate) effective_linear_rgb: [f32; 3],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum EnvironmentFrameStatus {
    Available,
    Interior,
    Unavailable,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct EnvironmentFrameDiagnostics {
    pub(crate) eye_below_planet_radius: bool,
    pub(crate) clear_sky_model_revision: u32,
    pub(crate) star_frame: Option<StarFrameStamp>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct StarFrameStamp {
    pub(crate) context: WorldContext,
    pub(crate) clock_revision: ClockRevision,
    pub(crate) utc_day: i64,
    pub(crate) epoch_revision: u64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct EnvironmentFrame {
    pub(crate) context: Option<WorldContext>,
    pub(crate) descriptor: Option<EnvironmentDescriptor>,
    pub(crate) clock_revision: Option<ClockRevision>,
    pub(crate) sky: Option<SkyState>,
    pub(crate) observer: Option<ObserverFrame>,
    pub(crate) render_view: RenderViewFrame,
    pub(crate) global_sun: Option<GlobalSunFrame>,
    pub(crate) local_sun: Option<LocalBodyFrame>,
    pub(crate) local_moon: Option<LocalBodyFrame>,
    pub(crate) diagnostics: EnvironmentFrameDiagnostics,
    pub(crate) status: EnvironmentFrameStatus,
    pub(crate) camera_uniform: CameraUniform,
    pub(crate) environment_uniform: EnvironmentUniform,
}

impl EnvironmentFrame {
    pub(crate) fn from_sky_state(
        sky: &SkyState,
        observer: ObserverFrame,
        context: WorldContext,
        clock_revision: ClockRevision,
        descriptor: EnvironmentDescriptor,
        render_view: RenderViewFrame,
        settings: EnvironmentSettings,
    ) -> Result<Self, EnvironmentFrameError> {
        adapter::from_sky_state(
            sky,
            observer,
            context,
            clock_revision,
            descriptor,
            render_view,
            settings,
        )
    }

    pub(crate) fn interior(
        context: WorldContext,
        clock_revision: ClockRevision,
        descriptor: EnvironmentDescriptor,
        render_view: RenderViewFrame,
    ) -> Result<Self, EnvironmentFrameError> {
        adapter::interior(context, clock_revision, descriptor, render_view)
    }

    pub(crate) fn unavailable(render_view: RenderViewFrame) -> Self {
        adapter::unavailable(render_view)
    }
}

pub(crate) fn body_frame(
    body: CelestialBodyState,
    color: [f32; 3],
    scale: f32,
    visible_fraction: f32,
) -> LocalBodyFrame {
    LocalBodyFrame {
        geometric_direction_world: body.geometric_direction_world,
        apparent_direction_world: body.apparent_direction_world,
        geometric_altitude_rad: body.geometric_altitude_rad,
        apparent_altitude_rad: body.apparent_altitude_rad,
        angular_radius_rad: body.angular_radius_rad,
        distance_m: body.distance_m,
        visible_fraction,
        effective_linear_rgb: color.map(|channel| channel * scale * visible_fraction),
    }
}
