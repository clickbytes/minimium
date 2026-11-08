//! CPU-only meteor contract types validated at the boundary (ADR 0013 § 3-4).
//!
//! Physics lives in physical metres on world-aligned axes centred on the planet;
//! every conversion to game or render units is owned by [`crate::meteors::coords`].
//! The types carry no `wgpu`, window, protocol, storage or lifetime dependencies,
//! so the terminal `--no-default-features` build compiles them unchanged.
//! `Tracked` is an input marker only: no server producer, message, entity or save
//! field exists for it in this epic.

/// Producer semantics tag carried by every sample.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(u8)]
pub enum MeteorSource {
    /// Client-local cosmetic meteor (ambient dust, streaks, fragments, trails).
    Cosmetic = 0,
    /// Reserved input seam for a future tracked meteoroid adapter; never produced.
    Tracked = 1,
}

impl MeteorSource {
    /// Stable diagnostics label.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Cosmetic => "cosmetic",
            Self::Tracked => "tracked",
        }
    }
}

/// Lifecycle phase declared by the producer; presentation-only.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(u8)]
pub enum MeteorPhase {
    Incoming = 0,
    Peak = 1,
    Fading = 2,
}

impl MeteorPhase {
    /// Stable diagnostics label.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Incoming => "incoming",
            Self::Peak => "peak",
            Self::Fading => "fading",
        }
    }
}

/// Identity of every meteor-side sample: importance ties break by ascending id.
pub type MeteorId = u64;

/// Everything that renders or lights disappears beyond this |position| bound;
/// authored planets stay inside 10^7 m, so 10^10 is a safe outer envelope.
pub const POSITION_ABS_M_MAX: f64 = 1.0e10;
/// Far beyond the fastest observed meteoroid speed (~72 km/s relative to ground).
pub const VELOCITY_M_S_MAX: f64 = 100_000.0;
/// Visual core radius envelope; keeps every radius exactly representable in f32.
pub const RADIUS_PHYS_M_MAX: f64 = 2_048.0;
/// Linear HDR luminous-power envelope far above any observed bolide intensity.
pub const INTENSITY_MAX: f64 = 1.0e12;
/// Actionable maximum distance of one transient light centre.
pub const RANGE_M_MAX: f64 = 1.0e7;
/// A transient light never outlives five minutes; shorter values stay valid.
pub const TTL_S_MAX: f64 = 300.0;

/// Boundary validation failure with the offending named domain.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MeteorContractError {
    /// A required number was NaN, infinite or below the stated floor.
    NonFinite(&'static str),
    /// A value exceeded the named hard cap from this module.
    OutOfRange(&'static str),
}

impl core::fmt::Display for MeteorContractError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::NonFinite(domain) => write!(f, "meteor contract: non-finite {domain}"),
            Self::OutOfRange(domain) => write!(f, "meteor contract: out of range {domain}"),
        }
    }
}

impl std::error::Error for MeteorContractError {}

/// One observed or declared meteor core in physical units (ADR 0013 § 3).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MeteorSample {
    /// Deterministic identity; importance ties break by ascending id.
    pub id: MeteorId,
    /// Producer marker; `Tracked` implies no server producer exists.
    pub source: MeteorSource,
    /// Client-monotonic birth in seconds (`ANIMATION_EPOCH` convention).
    pub started_mono_s: f64,
    /// World-aligned, planet-centred position in physical metres.
    pub position_phys_m: [f64; 3],
    /// World-aligned physical velocity in m/s.
    pub velocity_phys_ms: [f64; 3],
    /// Emission colour in linear HDR before exposure, components in `[0, 1]`.
    pub linear_rgb: [f64; 3],
    /// Scalar emission power in linear HDR units.
    pub intensity: f64,
    /// Effective emitted radius in physical metres.
    pub radius_phys_m: f64,
    /// Producer-declared lifecycle phase.
    pub phase: MeteorPhase,
}

impl MeteorSample {
    /// Validates every field against the hard caps before accepting the sample.
    #[allow(clippy::too_many_arguments)] // single validated boundary constructor
    pub fn new(
        id: MeteorId,
        source: MeteorSource,
        started_mono_s: f64,
        position_phys_m: [f64; 3],
        velocity_phys_ms: [f64; 3],
        linear_rgb: [f64; 3],
        intensity: f64,
        radius_phys_m: f64,
        phase: MeteorPhase,
    ) -> Result<Self, MeteorContractError> {
        let sample = Self {
            id,
            source,
            started_mono_s,
            position_phys_m,
            velocity_phys_ms,
            linear_rgb,
            intensity,
            radius_phys_m,
            phase,
        };
        sample.validate()?;
        Ok(sample)
    }

    /// Rechecks a mutated sample; contract invalidity drops the sample later.
    pub fn validate(&self) -> Result<(), MeteorContractError> {
        check_nonnegative("started_mono_s", self.started_mono_s)?;
        check_bounded("position_phys_m", self.position_phys_m, POSITION_ABS_M_MAX)?;
        check_bounded("velocity_phys_ms", self.velocity_phys_ms, VELOCITY_M_S_MAX)?;
        check_unit("linear_rgb", self.linear_rgb)?;
        check_between("intensity", self.intensity, 0.0, INTENSITY_MAX)?;
        check_between("radius_phys_m", self.radius_phys_m, 0.0, RADIUS_PHYS_M_MAX)?;
        Ok(())
    }
}

/// One trail geometry point for a core, with an explicit monotonic timestamp.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MeteorTrailSample {
    /// Owning core identity; ties break by (`id`, ascending `serial`).
    pub id: MeteorId,
    /// Strictly monotonic sequence inside the owning core.
    pub serial: u32,
    /// Client-monotonic capture time in seconds.
    pub at_mono_s: f64,
    /// World-aligned, planet-centred position in physical metres.
    pub position_phys_m: [f64; 3],
    /// Trail half-width in physical metres.
    pub thickness_phys_m: f64,
    /// Opacity in `[0, 1]`.
    pub opacity: f64,
}

impl MeteorTrailSample {
    /// Validates the geometry and opacity bound.
    pub fn new(
        id: MeteorId,
        serial: u32,
        at_mono_s: f64,
        position_phys_m: [f64; 3],
        thickness_phys_m: f64,
        opacity: f64,
    ) -> Result<Self, MeteorContractError> {
        let sample = Self {
            id,
            serial,
            at_mono_s,
            position_phys_m,
            thickness_phys_m,
            opacity,
        };
        sample.validate()?;
        Ok(sample)
    }

    /// Rechecks a mutated sample.
    pub fn validate(&self) -> Result<(), MeteorContractError> {
        check_nonnegative("at_mono_s", self.at_mono_s)?;
        check_bounded("position_phys_m", self.position_phys_m, POSITION_ABS_M_MAX)?;
        check_between(
            "thickness_phys_m",
            self.thickness_phys_m,
            0.0,
            RADIUS_PHYS_M_MAX,
        )?;
        check_between("opacity", self.opacity, 0.0, 1.0)?;
        Ok(())
    }
}

/// One emitting centre feeding the transient-light path (uniform arrives E047-09).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TransientLightSample {
    /// Deterministic identity; received-luminance ties break by ascending id.
    pub id: MeteorId,
    /// World-aligned, planet-centred position in physical metres.
    pub position_phys_m: [f64; 3],
    /// Emission colour in linear HDR, components in `[0, 1]`.
    pub linear_rgb: [f64; 3],
    /// Scalar luminous intensity before the single shared exposure path.
    pub luminous_intensity: f64,
    /// Actionable maximum lighting distance in physical metres.
    pub range_m: f64,
    /// Remaining client-monotonic life in seconds.
    pub ttl_s: f64,
}

impl TransientLightSample {
    /// Validates light colour, power, range and time-to-live.
    pub fn new(
        id: MeteorId,
        position_phys_m: [f64; 3],
        linear_rgb: [f64; 3],
        luminous_intensity: f64,
        range_m: f64,
        ttl_s: f64,
    ) -> Result<Self, MeteorContractError> {
        let sample = Self {
            id,
            position_phys_m,
            linear_rgb,
            luminous_intensity,
            range_m,
            ttl_s,
        };
        sample.validate()?;
        Ok(sample)
    }

    /// Rechecks a mutated sample.
    pub fn validate(&self) -> Result<(), MeteorContractError> {
        check_bounded("position_phys_m", self.position_phys_m, POSITION_ABS_M_MAX)?;
        check_unit("linear_rgb", self.linear_rgb)?;
        check_between(
            "luminous_intensity",
            self.luminous_intensity,
            0.0,
            INTENSITY_MAX,
        )?;
        check_between("range_m", self.range_m, 0.0, RANGE_M_MAX)?;
        check_between("ttl_s", self.ttl_s, 0.0, TTL_S_MAX)?;
        Ok(())
    }
}

/// Rejects NaN, infinite values and anything below `floor`.
fn check_nonnegative(domain: &'static str, value: f64) -> Result<(), MeteorContractError> {
    check_between(domain, value, 0.0, f64::INFINITY)
}

/// Rejects NaN, infinite values and component magnitudes above `cap`.
///
/// Positions and velocities live on world-aligned axes centred on the planet,
/// so every component carries a sign; the hard bound applies to `|value|`.
fn check_bounded(
    domain: &'static str,
    values: [f64; 3],
    cap: f64,
) -> Result<(), MeteorContractError> {
    for value in values {
        let magnitude = value.abs();
        match (magnitude.is_finite(), magnitude <= cap) {
            (true, true) => {}
            (true, false) => return Err(MeteorContractError::OutOfRange(domain)),
            (false, _) => return Err(MeteorContractError::NonFinite(domain)),
        }
    }
    Ok(())
}

/// Rejects NaN, infinite values and components outside `[0, 1]`.
fn check_unit(domain: &'static str, values: [f64; 3]) -> Result<(), MeteorContractError> {
    for value in values {
        check_between(domain, value, 0.0, 1.0)?;
    }
    Ok(())
}

/// Rejects values outside `[low, high]` including NaN and infinities.
fn check_between(
    domain: &'static str,
    value: f64,
    low: f64,
    high: f64,
) -> Result<(), MeteorContractError> {
    match (value.is_finite(), value >= low, value <= high) {
        (true, true, true) => Ok(()),
        (true, _, _) => Err(MeteorContractError::OutOfRange(domain)),
        (false, _, _) => Err(MeteorContractError::NonFinite(domain)),
    }
}
