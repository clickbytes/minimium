//! A bounded frame of light sources independent of any emission producer.
use super::{LightContractError, TransientLightMoment, TransientLightSample, CAPACITY};
use crate::planet_coordinates::{physical_position_relative_m, PlanetSurfaceScale};
/// Per-frame adaptation context resolved by the caller for the current view.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TransientLightContext {
    /// World-anchored camera eye in game metres (`RenderViewFrame`).
    pub camera_eye_world_m: [f64; 3],
    /// Current floating render origin in game metres (`RenderViewFrame`).
    pub render_origin_m: [f64; 3],
    /// Game planet radius of the current view (`RenderViewFrame`).
    pub game_radius_m: f64,
    /// Shared vertical mapping of the current manifest (Earth and procedural).
    pub surface_scale: PlanetSurfaceScale,
}

#[derive(Clone, Copy, Debug)]
pub struct TransientLightFrame {
    context: Option<TransientLightContext>,
    lights: [TransientLightMoment; CAPACITY],
    light_count: usize,
    cloud_occlusion: bool,
}

impl TransientLightFrame {
    pub const fn empty() -> Self {
        Self {
            context: None,
            lights: [EMPTY_LIGHT; CAPACITY],
            light_count: 0,
            cloud_occlusion: false,
        }
    }
    pub fn new(context: TransientLightContext) -> Result<Self, LightContractError> {
        if !context
            .camera_eye_world_m
            .iter()
            .chain(&context.render_origin_m)
            .all(|v| v.is_finite())
            || !context.game_radius_m.is_finite()
            || !(0..3)
                .all(|i| (context.camera_eye_world_m[i] - context.render_origin_m[i]).is_finite())
            || !context.surface_scale.physical_altitude_m(0.0).is_finite()
            || !context.surface_scale.physical_altitude_m(1.0).is_finite()
        {
            return Err(LightContractError::NonFinite("context"));
        }
        if context.game_radius_m < 0.0 {
            return Err(LightContractError::OutOfRange("game_radius_m"));
        }
        Ok(Self {
            context: Some(context),
            cloud_occlusion: true,
            ..Self::empty()
        })
    }
    pub fn context(&self) -> Option<TransientLightContext> {
        self.context
    }
    pub fn lights(&self) -> &[TransientLightMoment] {
        &self.lights[..self.light_count]
    }
    pub fn cloud_occlusion(&self) -> bool {
        self.cloud_occlusion
    }
    pub fn with_cloud_occlusion(mut self, enabled: bool) -> Self {
        self.cloud_occlusion = enabled;
        self
    }
    // Producer adapter resolves its final emitting geometry after source selection.
    pub(crate) fn resolve_radii(&mut self, radius: impl Fn(u64) -> f32) {
        for light in &mut self.lights[..self.light_count] {
            let value = radius(light.id);
            if value.is_finite() && (0.0..=2048.0).contains(&value) {
                light.radius_m = value.max(0.01);
            }
        }
    }
    /// Adapts one validated transient light and inserts it by received
    /// luminance with the identity tie-breaker. Returns `false` (nothing
    /// changed) while disabled, on contract violations or on a duplicate
    /// identity; when the frame is full the lowest-ranked light is evicted
    /// deterministically to make room.
    pub fn push(&mut self, sample: TransientLightSample) -> bool {
        if self.context.is_none() {
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
            radius_m: sample.radius_m as f32,
        };
        if !candidate.is_finite_contract() {
            return false;
        }
        self.insert_light(candidate)
    }

    /// Inserts a light by `(received luminance desc, id asc)`; a full frame
    /// evicts the lowest-ranked slot first. Duplicates rejected earlier.
    fn insert_light(&mut self, candidate: TransientLightMoment) -> bool {
        let Some(context) = self.context else {
            return false;
        };
        let slot_rank =
            |slot: &TransientLightMoment| crate::transient_lighting::score(slot, context);
        let candidate_rank = slot_rank(&candidate);
        if candidate_rank <= 0.0 || self.lights().iter().any(|light| light.id == candidate.id) {
            return false;
        }
        if self.light_count == CAPACITY {
            let lowest = &self.lights[CAPACITY - 1];
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

    /// Maps a planet-centred physical position to world game metres with the
    /// shared surface scale, then subtracts the floating origin in `f64` and
    /// returns the `f32` render value. `None` at the planet centre (undefined
    /// direction) or physically below the game planet radius; producers cull
    /// far-away sources before the push anyway.
    fn relative_world_position(&self, position_phys_m: [f64; 3]) -> Option<[f32; 3]> {
        let context = self.context?;
        physical_position_relative_m(
            position_phys_m,
            context.game_radius_m,
            context.surface_scale,
            context.render_origin_m,
        )
    }
}

const EMPTY_LIGHT: TransientLightMoment = TransientLightMoment {
    id: 0,
    position_rel_m: [0.0; 3],
    linear_rgb: [0.0; 3],
    luminous_intensity: 0.0,
    range_m: 0.0,
    ttl_s: 0.0,
    radius_m: 0.0,
};
