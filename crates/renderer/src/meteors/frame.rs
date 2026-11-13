//! The bounded, non-allocating per-frame meteor input (ADR 0013 § 3, § 6).
//!
//! `MeteorFrame` is the per-frame CPU interface between any local producer
//! (ambient cosmetic runtime arrives E047-02) and the renderer passes
//! (emission layers E047-03, transient-light uniform E047-09). Validated
//! samples enter here as physical-metre `f64` values and leave adapted through
//! [`crate::meteors::coords`] as origin-relative `f32` render geometry. All
//! arrays are fixed, sorted and never allocated on the render path: cores by
//! `(importance desc, id asc)`, trails by `(id asc, serial asc)` and lights by
//! `(received luminance desc, id asc)`. Pushes are rejected in disabled states,
//! above the caps or on any contract violation; nothing panics and nothing
//! stores NaN. With all counts zero the frame costs the renderer no extra work.
//!
//! Producer seam (dormant by design until E047-02): the runtime rebuilds the
//! frame each frame for the current floating origin, so an origin/world/manifest
//! change or reconnect cannot carry stale absolute values. No server producer
//! exists; `Tracked` stays an input marker only.

use crate::meteors::coords::{render_relative_m, PlanetSurfaceScale};
use crate::meteors::model::{MeteorId, MeteorSample, MeteorTrailSample, TransientLightSample};
use crate::meteors::{
    MeteorCore, MeteorDisabledReason, MeteorFrameState, MeteorRenderStatus, TrailPoint,
    TransientLightMoment, MAX_METEOR_CORES, MAX_TRAIL_POINTS, MAX_TRAIL_POINTS_PER_CORE,
    MAX_TRANSIENT_LIGHTS,
};

/// Squared-distance floor below the importance denominators to keep them
/// finite at zero separation.
const IMPORTANCE_DISTANCE_FLOOR_M2: f64 = 1.0;

/// Per-frame adaptation context resolved by the caller for the current view.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MeteorFrameContext {
    /// World-anchored camera eye in game metres (`RenderViewFrame`).
    pub camera_eye_world_m: [f64; 3],
    /// Current floating render origin in game metres (`RenderViewFrame`).
    pub render_origin_m: [f64; 3],
    /// Game planet radius of the current view (`RenderViewFrame`).
    pub game_radius_m: f64,
    /// Shared vertical mapping of the current manifest (Earth and procedural).
    pub surface_scale: PlanetSurfaceScale,
}

/// Bounded per-frame meteor input consumed by the renderer pass set.
#[derive(Clone, Copy, Debug)]
pub struct MeteorFrame {
    visibility: super::visibility::VisibilityPolicy,
    state: MeteorFrameState,
    render_origin_m: [f64; 3],
    game_radius_m: f64,
    surface_scale: PlanetSurfaceScale,
    /// Eye relative to the render origin in `f64`; drives the importance rank.
    eye_rel_m: [f64; 3],
    cores: [MeteorCore; MAX_METEOR_CORES],
    core_count: usize,
    trails: [TrailPoint; MAX_TRAIL_POINTS],
    trail_count: usize,
    lights: [TransientLightMoment; MAX_TRANSIENT_LIGHTS],
    light_count: usize,
}

impl MeteorFrame {
    pub fn with_quality(mut self, quality: crate::quality::SkyQuality) -> Self {
        self.visibility = super::visibility::VisibilityPolicy::from_quality(quality);
        self
    }

    pub fn minimum_height_m(&self) -> f64 {
        self.visibility.minimum_height_m()
    }

    pub fn head_visible(&self, core: &MeteorCore) -> bool {
        let eye = std::array::from_fn(|i| self.render_origin_m[i] + self.eye_rel_m[i]);
        let source =
            std::array::from_fn(|i| self.render_origin_m[i] + f64::from(core.position_rel_m[i]));
        super::visibility::head_visible(eye, source, self.game_radius_m, f64::from(core.radius_m))
    }
    pub fn cores(&self) -> &[MeteorCore] {
        &self.cores[..self.core_count]
    }

    pub fn trails(&self) -> &[TrailPoint] {
        &self.trails[..self.trail_count]
    }

    /// Inverse of the producer adapter, used by the high-atmosphere GPU gate.
    pub fn physical_height(&self, relative: [f32; 3]) -> f64 {
        let world = std::array::from_fn(|i| self.render_origin_m[i] + f64::from(relative[i]));
        self.surface_scale
            .physical_altitude_m(magnitude(world).unwrap_or(0.0) - self.game_radius_m)
    }
    /// Ready frame scoped to one prepared view; contains no meteors yet.
    /// A non-finite context degrades to the explicit disabled state instead of
    /// panicking or storing NaN.
    pub fn ready(context: MeteorFrameContext) -> Self {
        let eye_rel_m = [
            context.camera_eye_world_m[0] - context.render_origin_m[0],
            context.camera_eye_world_m[1] - context.render_origin_m[1],
            context.camera_eye_world_m[2] - context.render_origin_m[2],
        ];
        if !eye_rel_m.iter().all(|value| value.is_finite())
            || !context
                .render_origin_m
                .iter()
                .all(|value| value.is_finite())
            || !context
                .camera_eye_world_m
                .iter()
                .all(|value| value.is_finite())
            || !context.game_radius_m.is_finite()
            || context.game_radius_m < 0.0
        {
            return Self::disabled(MeteorDisabledReason::NoEnvironment);
        }
        Self {
            visibility: super::visibility::VisibilityPolicy::FiniteDistance,
            state: MeteorFrameState::Ready,
            render_origin_m: context.render_origin_m,
            game_radius_m: context.game_radius_m,
            surface_scale: context.surface_scale,
            eye_rel_m,
            cores: [EMPTY_CORE; MAX_METEOR_CORES],
            core_count: 0,
            trails: [EMPTY_TRAIL; MAX_TRAIL_POINTS],
            trail_count: 0,
            lights: [EMPTY_LIGHT; MAX_TRANSIENT_LIGHTS],
            light_count: 0,
        }
    }

    /// Explicit disabled frame; the environment currently forbids meteors.
    pub const fn disabled(reason: MeteorDisabledReason) -> Self {
        // The neutral unit scale is only a placeholder carrier for the never
        // read context; nothing past the state leaves a disabled frame.
        Self {
            visibility: super::visibility::VisibilityPolicy::HighAtmosphereOnly,
            state: MeteorFrameState::Disabled(reason),
            render_origin_m: [0.0; 3],
            game_radius_m: 0.0,
            surface_scale: PlanetSurfaceScale::UNIT,
            eye_rel_m: [0.0; 3],
            cores: [EMPTY_CORE; MAX_METEOR_CORES],
            core_count: 0,
            trails: [EMPTY_TRAIL; MAX_TRAIL_POINTS],
            trail_count: 0,
            lights: [EMPTY_LIGHT; MAX_TRANSIENT_LIGHTS],
            light_count: 0,
        }
    }

    /// Current explicit state.
    pub const fn state(self) -> MeteorFrameState {
        self.state
    }

    /// Per-frame status consumed by the GPU submission and the diagnostics.
    pub fn render_status(self) -> MeteorRenderStatus {
        match self.state {
            MeteorFrameState::Disabled(reason) => MeteorRenderStatus::Disabled(reason),
            MeteorFrameState::Ready => {
                let core_count = self.core_count;
                let trail_count = self.trail_count;
                let light_count = self.light_count;
                let contract_ok = self.cores[..core_count]
                    .iter()
                    .all(|core| core.is_finite_contract())
                    && self.trails[..trail_count]
                        .iter()
                        .all(|trail| trail.is_finite_contract())
                    && self.lights[..light_count]
                        .iter()
                        .all(|light| light.is_finite_contract());
                MeteorRenderStatus::Ready {
                    cores: core_count as u16,
                    trails: trail_count as u16,
                    lights: light_count as u16,
                    contract_ok,
                }
            }
        }
    }

    /// Adapts one validated physical sample through [`crate::meteors::coords`]
    /// and inserts it with deterministic priority. Returns `false` (nothing
    /// changed) while disabled, on any contract violation, a below-surface
    /// position or a duplicate identity.
    pub fn push_core(&mut self, sample: MeteorSample) -> bool {
        if self.state != MeteorFrameState::Ready {
            return false;
        }
        if sample.validate().is_err() {
            return false;
        }
        let Some(position_rel_m) = self.relative_world_position(sample.position_phys_m) else {
            return false;
        };
        let candidate = MeteorCore {
            id: sample.id,
            source: sample.source,
            phase: sample.phase,
            position_rel_m,
            radius_m: sample.radius_phys_m.max(0.0) as f32,
            linear_rgb: sample.linear_rgb.map(|value| value as f32),
            intensity: sample.intensity.max(0.0) as f32,
        };
        if !candidate.is_finite_contract() {
            return false;
        }
        self.insert_core(candidate)
    }

    /// Adapts one validated trail sample and orders it by
    /// `(id asc, serial asc)` for its owning core; the frame accepts no trail
    /// beyond 8 per core or 128 in total. Returns `false` (nothing changed)
    /// while disabled, on contract violations, when the owning core is absent,
    /// the caps are full, or on a duplicate `(id, serial)`.
    pub fn push_trail(&mut self, sample: MeteorTrailSample) -> bool {
        if self.state != MeteorFrameState::Ready {
            return false;
        }
        if sample.validate().is_err() {
            return false;
        }
        let Some(position_rel_m) = self.relative_world_position(sample.position_phys_m) else {
            return false;
        };
        let candidate = TrailPoint {
            id: sample.id,
            serial: sample.serial,
            position_rel_m,
            thickness_m: sample.thickness_phys_m.max(0.0) as f32,
            opacity: sample.opacity as f32,
            kind: sample.kind,
            intensity: sample.intensity as f32,
        };
        if !candidate.is_finite_contract()
            || !self.core_exists(candidate.id)
            || self.trail_count == MAX_TRAIL_POINTS
            || self.trails_of_core(candidate.id) >= MAX_TRAIL_POINTS_PER_CORE
            || self.trail_exists(candidate.id, candidate.serial)
        {
            return false;
        }
        self.insert_trail(candidate)
    }

    /// Adapts one validated transient light and inserts it by received
    /// luminance with the identity tie-breaker. Returns `false` (nothing
    /// changed) while disabled, on contract violations or on a duplicate
    /// identity; when the frame is full the lowest-ranked light is evicted
    /// deterministically to make room.
    pub fn push_light(&mut self, sample: TransientLightSample) -> bool {
        if self.state != MeteorFrameState::Ready {
            return false;
        }
        if sample.validate().is_err() {
            return false;
        }
        let Some(position_rel_m) = self.relative_world_position(sample.position_phys_m) else {
            return false;
        };
        let candidate = TransientLightMoment {
            id: sample.id,
            position_rel_m,
            linear_rgb: sample.linear_rgb.map(|value| value as f32),
            luminous_intensity: sample.luminous_intensity.max(0.0),
            range_m: sample.range_m.max(0.0) as f32,
            ttl_s: sample.ttl_s.max(0.0) as f32,
        };
        if !candidate.is_finite_contract() {
            return false;
        }
        self.insert_light(candidate)
    }

    /// Inserts a core by `(importance desc, id asc)`; a full frame evicts the
    /// lowest-ranked slot first. Duplicate identities are rejected earlier.
    fn insert_core(&mut self, candidate: MeteorCore) -> bool {
        let eye_rel_m = self.eye_rel_m;
        let slot_rank =
            |slot: &MeteorCore| received(f64::from(slot.intensity), slot.position_rel_m, eye_rel_m);
        let candidate_rank = received(
            f64::from(candidate.intensity),
            candidate.position_rel_m,
            eye_rel_m,
        );
        if self.core_count == MAX_METEOR_CORES {
            let lowest = &self.cores[MAX_METEOR_CORES - 1];
            let keep_new = candidate_rank > slot_rank(lowest)
                || (candidate_rank == slot_rank(lowest) && candidate.id < lowest.id);
            if !keep_new {
                return false;
            }
            let removed_id = lowest.id;
            let mut kept = 0;
            for index in 0..self.trail_count {
                if self.trails[index].id != removed_id {
                    self.trails[kept] = self.trails[index];
                    kept += 1;
                }
            }
            self.trail_count = kept;
            self.core_count -= 1;
        }
        let mut insert_at = self.core_count;
        for index in 0..self.core_count {
            let rank = slot_rank(&self.cores[index]);
            if candidate_rank > rank
                || (candidate_rank == rank && candidate.id < self.cores[index].id)
            {
                insert_at = index;
                break;
            }
        }
        let mut shift = self.core_count;
        while shift > insert_at {
            self.cores[shift] = self.cores[shift - 1];
            shift -= 1;
        }
        self.cores[insert_at] = candidate;
        self.core_count += 1;
        true
    }

    /// Inserts a light by `(received luminance desc, id asc)`; a full frame
    /// evicts the lowest-ranked slot first. Duplicates rejected earlier.
    fn insert_light(&mut self, candidate: TransientLightMoment) -> bool {
        let eye_rel_m = self.eye_rel_m;
        let slot_rank = |slot: &TransientLightMoment| {
            received(slot.luminous_intensity, slot.position_rel_m, eye_rel_m)
        };
        let candidate_rank = received(
            candidate.luminous_intensity,
            candidate.position_rel_m,
            eye_rel_m,
        );
        if self.light_count == MAX_TRANSIENT_LIGHTS {
            let lowest = &self.lights[MAX_TRANSIENT_LIGHTS - 1];
            let keep_new = candidate_rank > slot_rank(lowest)
                || (candidate_rank == slot_rank(lowest) && candidate.id < lowest.id);
            if !keep_new {
                return false;
            }
            self.light_count -= 1;
        }
        let mut insert_at = self.light_count;
        for index in 0..self.light_count {
            let rank = slot_rank(&self.lights[index]);
            if candidate_rank > rank
                || (candidate_rank == rank && candidate.id < self.lights[index].id)
            {
                insert_at = index;
                break;
            }
        }
        let mut shift = self.light_count;
        while shift > insert_at {
            self.lights[shift] = self.lights[shift - 1];
            shift -= 1;
        }
        self.lights[insert_at] = candidate;
        self.light_count += 1;
        true
    }

    /// Inserts an ordered trail point; caps are checked by the caller.
    fn insert_trail(&mut self, candidate: TrailPoint) -> bool {
        let mut insert_at = self.trail_count;
        for index in 0..self.trail_count {
            if (self.trails[index].id, self.trails[index].serial) > (candidate.id, candidate.serial)
            {
                insert_at = index;
                break;
            }
        }
        let mut shift = self.trail_count;
        while shift > insert_at {
            self.trails[shift] = self.trails[shift - 1];
            shift -= 1;
        }
        self.trails[insert_at] = candidate;
        self.trail_count += 1;
        true
    }

    /// `true` while a core with this identity is stored.
    fn core_exists(self, id: MeteorId) -> bool {
        self.cores[..self.core_count]
            .iter()
            .any(|core| core.id == id)
    }

    /// `true` while this `(id, serial)` trail point is stored.
    fn trail_exists(self, id: MeteorId, serial: u32) -> bool {
        self.trails[..self.trail_count]
            .iter()
            .any(|trail| trail.id == id && trail.serial == serial)
    }

    /// Stored trail points of one core.
    fn trails_of_core(self, id: MeteorId) -> usize {
        self.trails[..self.trail_count]
            .iter()
            .filter(|trail| trail.id == id)
            .count()
    }

    /// Maps a planet-centred physical position to world game metres with the
    /// shared surface scale, then subtracts the floating origin in `f64` and
    /// returns the `f32` render value. `None` at the planet centre (undefined
    /// direction) or physically below the game planet radius; producers cull
    /// far-away sources before the push anyway.
    fn relative_world_position(&self, position_phys_m: [f64; 3]) -> Option<[f32; 3]> {
        let radius_phys_m = magnitude(position_phys_m)?;
        if radius_phys_m < self.game_radius_m {
            return None;
        }
        let direction = [
            position_phys_m[0] / radius_phys_m,
            position_phys_m[1] / radius_phys_m,
            position_phys_m[2] / radius_phys_m,
        ];
        let game_radial_m = self.game_radius_m
            + self
                .surface_scale
                .radial_offset_m(radius_phys_m - self.game_radius_m);
        let world_m = [
            direction[0] * game_radial_m,
            direction[1] * game_radial_m,
            direction[2] * game_radial_m,
        ];
        Some(render_relative_m(world_m, self.render_origin_m))
    }
}

/// Zero-valued slot constants; slots beyond the counts are never read.
const EMPTY_CORE: MeteorCore = MeteorCore {
    id: 0,
    source: crate::meteors::model::MeteorSource::Cosmetic,
    phase: crate::meteors::model::MeteorPhase::Incoming,
    position_rel_m: [0.0; 3],
    radius_m: 0.0,
    linear_rgb: [0.0; 3],
    intensity: 0.0,
};

const EMPTY_TRAIL: TrailPoint = TrailPoint {
    id: 0,
    serial: 0,
    position_rel_m: [0.0; 3],
    thickness_m: 0.0,
    opacity: 0.0,
    kind: crate::meteors::trail::TrailKind::Wake,
    intensity: 0.0,
};

const EMPTY_LIGHT: TransientLightMoment = TransientLightMoment {
    id: 0,
    position_rel_m: [0.0; 3],
    linear_rgb: [0.0; 3],
    luminous_intensity: 0.0,
    range_m: 0.0,
    ttl_s: 0.0,
};

/// `|v|`; `None` at zero length (the direction is undefined there).
fn magnitude(values: [f64; 3]) -> Option<f64> {
    let squared = values.iter().map(|value| value * value).sum::<f64>();
    (squared > 0.0).then(|| squared.sqrt())
}

/// Deterministic received-impact analogue: power over squared origin-relative
/// distance plus a finite floor, so a zero separation stays finite.
fn received(power: f64, position_rel_m: [f32; 3], eye_rel_m: [f64; 3]) -> f64 {
    let dx = f64::from(position_rel_m[0]) - eye_rel_m[0];
    let dy = f64::from(position_rel_m[1]) - eye_rel_m[1];
    let dz = f64::from(position_rel_m[2]) - eye_rel_m[2];
    let squared = dx * dx + dy * dy + dz * dz + IMPORTANCE_DISTANCE_FLOOR_M2;
    power / squared
}
