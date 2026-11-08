mod adapter;
mod clock;
mod halo;
pub mod observer;
mod orbit;
mod prepared;
mod runtime;

use minimium_astronomy::{CelestialBodyState, ObservedBody, SkyState};
use minimium_environment::sky::ObserverFrame;
use minimium_environment::{EnvironmentDescriptor, time::ClockRevision};
use minimium_protocol::WorldContext;
use minimium_renderer::gpu_frame::{CameraUniform, EnvironmentUniform};
use minimium_renderer::meteors::coords::PlanetSurfaceScale;
pub(crate) use minimium_renderer::{EnvironmentViewKind, RenderViewFrame, RenderViewInput};
use std::{error::Error, fmt};

pub(crate) use minimium_renderer::gpu_frame::{HaloSource, MAX_HALO_BODIES};

pub(crate) use minimium_renderer::profiles::LightingProfile as EnvironmentSettings;
pub(super) use prepared::{PreparedEnvironmentFrame, PreparedStarUpload};

pub(crate) struct SkyFrameInput {
    pub(crate) observer: ObserverFrame,
    pub(crate) context: WorldContext,
    pub(crate) clock_revision: ClockRevision,
    pub(crate) descriptor: EnvironmentDescriptor,
    pub(crate) render_view: RenderViewFrame,
    /// Manifest-resolved vertical scale at the camera reference instant;
    /// the meteor runtime reuses the identical mapping.
    pub(crate) meteor_surface_scale: PlanetSurfaceScale,
    pub(crate) observed_bodies: Vec<ObservedBody>,
    pub(crate) settings: EnvironmentSettings,
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

/// Reasons a prepared environment frame (sky, interior or unavailable) could
/// not be built or validated this frame.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum EnvironmentFrameError {
    DescriptorMismatch,
    SkyStateInvalid,
    ObserverMismatch,
    SettingsInvalid,
}

impl fmt::Display for EnvironmentFrameError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::DescriptorMismatch => "environment descriptor is incompatible with the sky",
            Self::SkyStateInvalid => "astronomy returned an invalid sky state",
            Self::ObserverMismatch => "observer and sky model revisions differ",
            Self::SettingsInvalid => "environment settings contain an invalid value",
        };
        formatter.write_str(message)
    }
}

impl Error for EnvironmentFrameError {}

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

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct EnvironmentFrame {
    pub(crate) context: Option<WorldContext>,
    pub(crate) descriptor: Option<EnvironmentDescriptor>,
    pub(crate) clock_revision: Option<ClockRevision>,
    pub(crate) sky: Option<SkyState>,
    pub(crate) observed_bodies: Vec<ObservedBody>,
    pub(crate) halo_sources: [HaloSource; MAX_HALO_BODIES],
    pub(crate) halo_count: u32,
    pub(crate) observer: Option<ObserverFrame>,
    pub(crate) render_view: RenderViewFrame,
    pub(crate) global_sun: Option<GlobalSunFrame>,
    pub(crate) local_sun: Option<LocalBodyFrame>,
    pub(crate) local_moon: Option<LocalBodyFrame>,
    pub(crate) diagnostics: EnvironmentFrameDiagnostics,
    pub(crate) status: EnvironmentFrameStatus,
    /// Manifest-resolved vertical scale for the meteor runtime; the neutral
    /// unit scale marks frames where meteors never run (interior/unavailable).
    pub(crate) meteor_surface_scale: PlanetSurfaceScale,
    pub(crate) camera_uniform: CameraUniform,
    pub(crate) environment_uniform: EnvironmentUniform,
}

impl EnvironmentFrame {
    pub(crate) fn from_sky_state(
        sky: &SkyState,
        input: SkyFrameInput,
    ) -> Result<Self, EnvironmentFrameError> {
        adapter::from_sky_state(sky, input)
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
