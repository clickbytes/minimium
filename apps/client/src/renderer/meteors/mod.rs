//! Client-local ambient meteor runtime (ADR 0013 § 2, § 5; issue E047-02).
//!
//! The runtime owns the complete manual event lifecycle: identifiers, seeds,
//! start times, durations, sources and the active limit. It runs on the client
//! monotonic clock (one `Instant` epoch; no second clock, thread, queue or
//! allocation on the render path), rebuilds one bounded
//! [`MeteorFrame`](minimium_renderer::meteors::MeteorFrame) per frame, and
//! never touches world, protocol, save, server or GPU state. Track derivation
//! is deterministic: the same seed produces the same flight basis, and seeds
//! mix only the monotonic birth time with the spawn counter. The automatic
//! ambient generator is E047-04; visible sky integration is E047-03.

use crate::client_commands::MeteorPreset;
use minimium_protocol::WorldContext;
use minimium_renderer::meteors::model::{MeteorId, MeteorPhase};
use std::time::Instant;

mod evolution;
mod presentation;
mod runtime;
mod spawner;
mod track;
mod trains;

const EVENT_ID_STRIDE: u64 = 32;

/// Manual diagnostic events stay far below the 16-core contract cap so the
/// automatic generator (E047-04) keeps headroom; `8 × 8` trail slots still fit
/// the 8-per-core frame budget exactly.
pub(crate) const MAX_ACTIVE_METEOR_EVENTS: usize = 8;

/// Per-event trail slots, equal to the contract's 8-per-core budget; each slot
/// has a deterministic capture time and is reconstructed in every fresh frame.
pub(crate) const TRAIL_SLOTS_PER_EVENT: usize =
    minimium_renderer::meteors::MAX_TRAIL_POINTS_PER_CORE;

/// Local presentation profiles in physical units (ADR 0013 § 3-4).
/// Mass and entry speed initialize the solver; radiance is computed from
/// deposited kinetic energy, not a preset fade curve.
#[derive(Clone, Copy, Debug)]
pub(crate) struct SpawnParams {
    /// Absolute track speed in physical m/s (contract cap 100 km/s).
    pub(super) speed_m_s: f64,
    /// Planned client-monotonic lifetime in seconds (contract cap 300 s).
    pub(super) duration_s: f64,
    /// Initial solid mass in kilograms.
    pub(super) mass_kg: f64,
    /// Trail half-width in physical metres.
    pub(super) trail_thickness_m: f64,
    /// Trail opacity in `[0, 1]`.
    pub(super) trail_opacity: f64,
    /// Optional transient light carried by the event (fireball-class only).
    pub(super) light: Option<LightParams>,
}

/// Transient-light parameters of one preset, validated by the contract push.
#[derive(Clone, Copy, Debug)]
pub(crate) struct LightParams {
    /// Actionable maximum lighting distance in physical metres.
    pub(super) range_m: f64,
}

impl SpawnParams {
    /// Constant preset table; `faint`/`bright`/`globe` labels match
    /// `MeteorPreset::parse` in the command layer.
    pub(super) const fn of(preset: MeteorPreset) -> Self {
        match preset {
            MeteorPreset::Faint => Self {
                speed_m_s: 18.0e3,
                duration_s: 2.2,
                mass_kg: 0.002,
                trail_thickness_m: 0.8,
                trail_opacity: 0.72,
                light: None,
            },
            MeteorPreset::Bright => Self {
                speed_m_s: 32.0e3,
                duration_s: 3.6,
                mass_kg: 0.5,
                trail_thickness_m: 1.6,
                trail_opacity: 0.9,
                light: Some(LightParams { range_m: 6.0e4 }),
            },
            MeteorPreset::Fragmenting => Self {
                duration_s: 1.0,
                ..Self::of(MeteorPreset::Fireball)
            },
            MeteorPreset::Fireball => Self {
                speed_m_s: 55.0e3,
                duration_s: 5.2,
                mass_kg: 100.0,
                trail_thickness_m: 3.2,
                trail_opacity: 1.0,
                light: Some(LightParams { range_m: 1.5e5 }),
            },
        }
    }
}

/// Last seen world scope; a change dismisses every pending event instantly.
pub(crate) enum ContextState {
    /// No valid environment frame has been built since runtime creation.
    Pending,
    /// The environment currently admits meteors for this world scope.
    Available { context: WorldContext },
    /// Explicit zero-cost state with the validated reason.
    Unavailable { reason: &'static str },
}

/// One deterministic flight basis in planet-centred physical metres.
#[derive(Clone, Copy, Debug)]
pub(super) struct EventBasis {
    /// Start position at planner time (world-aligned, planet-centred).
    pub(super) start_phys_m: [f64; 3],
    /// World-aligned physical velocity in m/s.
    pub(super) velocity_m_s: [f64; 3],
}

/// One active manual meteor event (lifecycle data, no geometry storage).
pub(crate) struct ActiveEvent {
    pub(super) id: MeteorId,
    pub(super) preset: MeteorPreset,
    pub(super) seed: u64,
    /// Client-monotonic birth time in seconds; lifetime and trail slots hang
    /// from this single datum so ages never drift between frames.
    pub(super) start_s: f64,
    pub(super) duration_s: f64,
    pub(super) params: SpawnParams,
    /// `None` until the first valid environment frame after the spawn; the
    /// basis initialization never memorizes a stale disabled view.
    pub(super) basis: Option<EventBasis>,
    /// Producer-declared phase of the last built frame.
    pub(super) phase: MeteorPhase,
    pub(super) automatic: bool,
    evolution: Option<evolution::Evolution>,
    trains: trains::Trains,
}

impl ActiveEvent {
    /// Constructs one paused event; the flight basis initializes later.
    fn new(
        id: MeteorId,
        preset: MeteorPreset,
        seed: u64,
        now_s: f64,
        history: evolution::Evolution,
        allow_long: bool,
    ) -> Self {
        let params = SpawnParams::of(preset);
        Self {
            id,
            preset,
            seed,
            start_s: now_s,
            duration_s: params.duration_s,
            params,
            basis: None,
            phase: MeteorPhase::Incoming,
            automatic: false,
            evolution: Some(history),
            trains: trains::Trains::new(seed, params.mass_kg >= 20.0, allow_long),
        }
    }
}

/// Client-local manual meteor lifetime state (ADR 0013 § 2).
pub(crate) struct MeteorRuntime {
    /// The single client monotonic datum (ADR 0013 § 5); every age derives
    /// from `epoch.elapsed()`, so no second clock exists.
    epoch: Instant,
    next_id: MeteorId,
    active: Vec<ActiveEvent>,
    spawned_total: u64,
    dismissed_total: u64,
    last_seed: u64,
    /// Last world scope seen on a valid frame, for planar change detection.
    context: Option<WorldContext>,
    context_state: ContextState,
    spawner: spawner::Spawner,
    history_pool: Vec<evolution::Evolution>,
    omitted_cores: u64,
}

impl Default for MeteorRuntime {
    fn default() -> Self {
        Self::new()
    }
}
