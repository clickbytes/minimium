//! Validated producer-neutral light samples and origin-relative moments.
const POSITION_ABS_M_MAX: f64 = 1.0e10;
const INTENSITY_MAX: f64 = 1.0e12;
const RANGE_M_MAX: f64 = 1.0e7;
const TTL_S_MAX: f64 = 300.0;
const RENDER_REL_M_MAX: f32 = 5.0e7;

/// Boundary validation failure with the offending named domain.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LightContractError {
    /// A required number was NaN, infinite or below the stated floor.
    NonFinite(&'static str),
    /// A value exceeded the named hard cap from this module.
    OutOfRange(&'static str),
}

impl core::fmt::Display for LightContractError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::NonFinite(domain) => write!(f, "light contract: non-finite {domain}"),
            Self::OutOfRange(domain) => write!(f, "light contract: out of range {domain}"),
        }
    }
}

impl std::error::Error for LightContractError {}

/// One emitting centre feeding the shared transient-light uniform.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TransientLightSample {
    /// Deterministic identity; received-luminance ties break by ascending id.
    pub id: u64,
    /// World-aligned, planet-centred position in physical metres.
    pub position_phys_m: [f64; 3],
    /// Emission colour in linear HDR, components in `[0, 1]`.
    pub linear_rgb: [f64; 3],
    /// Scene-linear radiant intensity (visible power / 4 pi), before exposure.
    pub luminous_intensity: f64,
    /// Actionable maximum lighting distance in physical metres.
    pub range_m: f64,
    /// Remaining client-monotonic life in seconds.
    pub ttl_s: f64,
    /// Effective emitting radius in physical metres.
    pub radius_m: f64,
}

impl TransientLightSample {
    /// Validates light colour, power, range and time-to-live.
    pub fn new(
        id: u64,
        position_phys_m: [f64; 3],
        linear_rgb: [f64; 3],
        luminous_intensity: f64,
        range_m: f64,
        ttl_s: f64,
        radius_m: f64,
    ) -> Result<Self, LightContractError> {
        let sample = Self {
            id,
            position_phys_m,
            linear_rgb,
            luminous_intensity,
            range_m,
            ttl_s,
            radius_m,
        };
        sample.validate()?;
        Ok(sample)
    }

    /// Rechecks a mutated sample.
    pub fn validate(&self) -> Result<(), LightContractError> {
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
        check_between("radius_m", self.radius_m, 0.0, 2048.0)?;
        Ok(())
    }
}

/// One render-side transient light moment in origin-relative `f32` render units.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TransientLightMoment {
    /// Deterministic identity; also the received-luminance tie-breaker.
    pub id: u64,
    /// `f32` metres relative to the render origin (never absolute world values).
    pub position_rel_m: [f32; 3],
    /// Emission colour, linear HDR, components in `[0, 1]`.
    pub linear_rgb: [f32; 3],
    /// Scene-linear radiant intensity (visible power / 4 pi), before exposure.
    pub luminous_intensity: f64,
    /// Actionable maximum lighting distance in physical metres.
    pub range_m: f32,
    /// Remaining client-monotonic life in seconds.
    pub ttl_s: f32,
    pub radius_m: f32,
}

impl TransientLightMoment {
    /// Contract check used by the per-frame render-status validation.
    pub fn is_finite_contract(self) -> bool {
        self.position_rel_m
            .iter()
            .all(|value| value.is_finite() && value.abs() <= RENDER_REL_M_MAX)
            && self
                .linear_rgb
                .iter()
                .all(|value| value.is_finite() && *value >= 0.0 && *value <= 1.0)
            && self.luminous_intensity.is_finite()
            && (0.0..=INTENSITY_MAX).contains(&self.luminous_intensity)
            && self.range_m.is_finite()
            && self.range_m >= 0.0
            && self.ttl_s.is_finite()
            && self.ttl_s >= 0.0
            && self.radius_m.is_finite()
            && (0.0..=2048.0).contains(&self.radius_m)
    }
}

/// Rejects NaN, infinite values and component magnitudes above `cap`.
///
/// Positions and velocities live on world-aligned axes centred on the planet,
/// so every component carries a sign; the hard bound applies to `|value|`.
fn check_bounded(
    domain: &'static str,
    values: [f64; 3],
    cap: f64,
) -> Result<(), LightContractError> {
    for value in values {
        let magnitude = value.abs();
        match (magnitude.is_finite(), magnitude <= cap) {
            (true, true) => {}
            (true, false) => return Err(LightContractError::OutOfRange(domain)),
            (false, _) => return Err(LightContractError::NonFinite(domain)),
        }
    }
    Ok(())
}

/// Rejects NaN, infinite values and components outside `[0, 1]`.
fn check_unit(domain: &'static str, values: [f64; 3]) -> Result<(), LightContractError> {
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
) -> Result<(), LightContractError> {
    match (value.is_finite(), value >= low, value <= high) {
        (true, true, true) => Ok(()),
        (true, _, _) => Err(LightContractError::OutOfRange(domain)),
        (false, _, _) => Err(LightContractError::NonFinite(domain)),
    }
}
