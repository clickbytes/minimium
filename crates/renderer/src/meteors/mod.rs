//! The bounded, non-allocating meteor input contract (ADR 0013 § 3, § 6).
//!
//! This module is the E047-01 CPU-only contract between any local producer and
//! the renderer pass set. [`crate::meteors::frame::MeteorFrame`] is the
//! per-frame input; [`model`] holds the validated physical samples and [`coords`]
//! the single physical ↔ game ↔ render-origin adapter. The frame carries fixed,
//! sorted arrays and never allocates on the render path; pushes are rejected in
//! disabled states, above the caps or on any contract violation, so nothing
//! panics and nothing stores NaN. The producer seam stays dormant by design
//! until E047-02; no server producer exists and `Tracked` is an input marker
//! only.

pub mod coords;
pub mod fragmentation;
pub mod kinematics;
pub mod model;
pub mod photometry;
pub mod trail;
pub mod visibility;

mod frame;

pub use frame::{MeteorFrame, MeteorFrameContext};

use crate::meteors::model::{MeteorId, MeteorPhase, MeteorSource};

/// Hard cap on bright cores rendered per frame (ADR 0013 § 6).
pub const MAX_METEOR_CORES: usize = 16;
/// Hard cap on transient light sources per frame (ADR 0013 § 6).
pub const MAX_TRANSIENT_LIGHTS: usize = 4;
/// Hard cap on trail points carried per frame (8 per core across 16 cores;
/// the bolid train uses 24 adaptive nodes per event, meteors stay shorter).
pub const MAX_TRAIL_POINTS: usize = 128;
/// Per-core trail sample cap enforced at push time.
pub const MAX_TRAIL_POINTS_PER_CORE: usize = MAX_TRAIL_POINTS / MAX_METEOR_CORES;
/// Contract-time dust geometry envelope; the dust layers added in E047-06
/// must never exceed this bound.
pub const MAX_DUST_SEGMENTS: usize = 256;
/// Per-meteor lifecycle history envelope for the client runtime (E047-02);
/// 512 samples at 40 ms spacing cover meteor lifetimes up to 20 s.
pub const MAX_METEOR_HISTORY_SAMPLES: usize = 512;
/// Render-relative magnitude bound; authored planets stay under 1.3e7 game
/// metres in diameter, so the overview camera stays in f32 comfort.
pub const RENDER_REL_M_MAX: f32 = 5.0e7;

/// Explicit contract state (ADR 0013 § 4 degraded states).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MeteorFrameState {
    /// Environment admits meteors; counts may still be all zero (no extra work).
    Ready,
    /// Environment forbids meteors; the frame is a zero-cost no-op.
    Disabled(MeteorDisabledReason),
}

/// Why the environment currently forbids any meteor output.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MeteorDisabledReason {
    /// Interior/dungeon profile (`SkyModel::None`).
    Interior,
    /// Missing context, world UTC or observer (`EnvironmentFrame::unavailable`).
    NoEnvironment,
}

impl MeteorDisabledReason {
    /// Stable diagnostics label.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Interior => "interior",
            Self::NoEnvironment => "no_environment",
        }
    }
}

/// What the bounded frame reports to the current consumer each frame.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MeteorRenderStatus {
    /// No frame has reached the consumer since startup or a skipped redraw.
    NotRendered,
    /// Environment forbids meteors; zero-cost no-op.
    Disabled(MeteorDisabledReason),
    /// Environment admits meteors; counts include the all-zero case, and
    /// `contract_ok` stays `false` if any stored entry lost its validity.
    Ready {
        cores: u16,
        trails: u16,
        lights: u16,
        contract_ok: bool,
    },
}

impl MeteorRenderStatus {
    /// Stable diagnostics label.
    pub const fn label(self) -> &'static str {
        match self {
            Self::NotRendered => "not_rendered",
            Self::Disabled(reason) => reason.label(),
            Self::Ready { .. } => "ready",
        }
    }

    /// Validated core count; zero in every non-ready state.
    pub const fn core_count(self) -> usize {
        match self {
            Self::Ready { cores, .. } => cores as usize,
            _ => 0,
        }
    }

    /// `true` while every stored entry still satisfies its contract check.
    pub const fn contract_ok(self) -> bool {
        match self {
            Self::Ready { contract_ok, .. } => contract_ok,
            _ => true,
        }
    }
}

/// One render-side meteor core in origin-relative `f32` render units.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MeteorCore {
    /// Deterministic identity; also the importance tie-breaker.
    pub id: MeteorId,
    /// Producer marker; `Tracked` implies no server producer exists.
    pub source: MeteorSource,
    /// Producer-declared lifecycle phase.
    pub phase: MeteorPhase,
    /// `f32` metres relative to the render origin (never absolute world values).
    pub position_rel_m: [f32; 3],
    /// Effective emitted radius in render metres.
    pub radius_m: f32,
    /// Emission colour, linear HDR, components in `[0, 1]`.
    pub linear_rgb: [f32; 3],
    /// Scalar emission power in linear HDR units before exposure.
    pub intensity: f32,
}

impl MeteorCore {
    /// Contract check used by the per-frame render-status validation.
    pub fn is_finite_contract(self) -> bool {
        self.position_rel_m
            .iter()
            .all(|value| value.is_finite() && value.abs() <= RENDER_REL_M_MAX)
            && self.radius_m.is_finite()
            && self.radius_m >= 0.0
            && self
                .linear_rgb
                .iter()
                .all(|value| value.is_finite() && *value >= 0.0 && *value <= 1.0)
            && self.intensity.is_finite()
            && self.intensity >= 0.0
    }
}

/// One render-side trail point in origin-relative `f32` render units.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TrailPoint {
    /// Owning core identity, first sort key.
    pub id: MeteorId,
    /// Owning core sequence, second sort key.
    pub serial: u32,
    /// `f32` metres relative to the render origin (never absolute world values).
    pub position_rel_m: [f32; 3],
    /// Trail half-width in render metres.
    pub thickness_m: f32,
    /// Opacity in `[0, 1]`.
    pub opacity: f32,
    pub kind: trail::TrailKind,
    pub intensity: f32,
}

impl TrailPoint {
    /// Contract check used by the per-frame render-status validation.
    pub fn is_finite_contract(self) -> bool {
        self.position_rel_m
            .iter()
            .all(|value| value.is_finite() && value.abs() <= RENDER_REL_M_MAX)
            && self.thickness_m.is_finite()
            && self.thickness_m >= 0.0
            && self.opacity.is_finite()
            && self.opacity >= 0.0
            && self.opacity <= 1.0
            && self.intensity.is_finite()
            && self.intensity >= 0.0
    }
}

/// One render-side transient light moment in origin-relative `f32` render units.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TransientLightMoment {
    /// Deterministic identity; also the received-luminance tie-breaker.
    pub id: MeteorId,
    /// `f32` metres relative to the render origin (never absolute world values).
    pub position_rel_m: [f32; 3],
    /// Emission colour, linear HDR, components in `[0, 1]`.
    pub linear_rgb: [f32; 3],
    /// Scalar luminous intensity before the single shared exposure path.
    pub luminous_intensity: f64,
    /// Actionable maximum lighting distance in render metres.
    pub range_m: f32,
    /// Remaining client-monotonic life in seconds.
    pub ttl_s: f32,
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
            && (0.0..=model::INTENSITY_MAX).contains(&self.luminous_intensity)
            && self.range_m.is_finite()
            && self.range_m >= 0.0
            && self.ttl_s.is_finite()
            && self.ttl_s >= 0.0
    }
}
