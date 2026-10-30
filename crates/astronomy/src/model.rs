use std::{error::Error, fmt, sync::Arc};

use minimium_core::{
    time::{ClockRevision, WorldTimeMs},
    world::{MembershipEpoch, WorldContentRevision, WorldInstanceId},
};

pub const ASTRONOMY_MODEL_REVISION: u32 = 1;
pub const MAX_STAR_RECORDS: usize = 500;
pub const EPHEMERIS_REVISION: u32 = 1;
pub const SUN_RADIUS_M: f64 = 695_700_000.0;
pub const MOON_MEAN_RADIUS_M: f64 = 1_737_400.0;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RefractionMode {
    None,
    Standard,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SkyError {
    InvalidTime,
    InvalidObserver,
    UpstreamStatus(i32),
    NonFinite,
    DegenerateVector,
    InvalidUpstreamValue,
    InvalidStarData,
    ContextMismatch,
    RevisionExhausted,
}

impl fmt::Display for SkyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidTime => formatter.write_str("UTC is outside the astronomy range"),
            Self::InvalidObserver => formatter.write_str("observer frame is invalid"),
            Self::UpstreamStatus(status) => {
                write!(formatter, "Astronomy Engine returned status {status}")
            }
            Self::NonFinite => formatter.write_str("astronomy returned a non-finite value"),
            Self::DegenerateVector => formatter.write_str("astronomy produced a zero vector"),
            Self::InvalidUpstreamValue => {
                formatter.write_str("astronomy returned an out-of-range value")
            }
            Self::InvalidStarData => formatter.write_str("star astrometry is invalid"),
            Self::ContextMismatch => formatter.write_str("sky state context does not match"),
            Self::RevisionExhausted => formatter.write_str("star epoch revision is exhausted"),
        }
    }
}

impl Error for SkyError {}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct AstronomyContextKey {
    instance: WorldInstanceId,
    content_revision: WorldContentRevision,
    membership_epoch: MembershipEpoch,
}

impl AstronomyContextKey {
    pub const fn new(
        instance: WorldInstanceId,
        content_revision: WorldContentRevision,
        membership_epoch: MembershipEpoch,
    ) -> Self {
        Self {
            instance,
            content_revision,
            membership_epoch,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SkySampleContext {
    pub utc_ms: WorldTimeMs,
    pub clock_revision: ClockRevision,
    pub world_context_key: AstronomyContextKey,
    pub observer_model_revision: u32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CelestialBodyState {
    pub geometric_direction_world: [f64; 3],
    pub apparent_direction_world: [f64; 3],
    pub geometric_altitude_rad: f64,
    pub apparent_altitude_rad: f64,
    pub azimuth_rad: f64,
    pub distance_m: f64,
    pub angular_radius_rad: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MoonState {
    pub body: CelestialBodyState,
    pub moon_to_sun_world: [f64; 3],
    pub moon_to_observer_world: [f64; 3],
    pub pole_world: [f64; 3],
    pub phase_angle_rad: f64,
    pub illuminated_fraction: f64,
    pub ecliptic_phase_rad: f64,
    pub waxing: bool,
    pub apparent_v_magnitude: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SkyState {
    pub utc_ms: WorldTimeMs,
    pub observer_model_revision: u32,
    pub sun: CelestialBodyState,
    pub moon: MoonState,
    /// Geocentric lunar position in the game world axes, in physical metres.
    pub geocentric_moon_position_world_m: [f64; 3],
    pub geocentric_sun_direction_world: [f64; 3],
    pub catalog_epoch_to_world: [[f64; 3]; 3],
    pub ephemeris_revision: u32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StarAstrometry {
    pub id: u32,
    pub position_eqj_pc: [f64; 3],
    pub velocity_eqj_pc_per_jyear: [f64; 3],
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StarDirection {
    pub id: u32,
    pub direction_eqj: [f64; 3],
}

#[derive(Clone, Debug, PartialEq)]
pub struct CorrectedStarDirections {
    pub star_epoch_revision: u64,
    pub directions: Arc<[StarDirection]>,
}
