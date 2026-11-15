//! Deterministic high-atmosphere diagnostic tracks.
use super::runtime::magnitude;
use super::{ActiveEvent, EventBasis};
use crate::renderer::environment::RenderViewFrame;
use minimium_renderer::meteors::{coords::PlanetSurfaceScale, model::POSITION_ABS_M_MAX};
use std::f64::consts::TAU;

impl ActiveEvent {
    /// Derives the deterministic flight basis from the current view and seed.
    /// Places the diagnostic track ahead of the camera above the cloud gate.
    /// Physical speed and duration determine its length; its descent is bounded
    /// to keep every preset within the high-atmosphere rendering stage.
    pub(super) fn init_basis(
        &mut self,
        view: &RenderViewFrame,
        scale: PlanetSurfaceScale,
    ) -> Result<(), &'static str> {
        let eye = view.camera_eye_world_m;
        if !eye.iter().all(|value| value.is_finite()) {
            return Err("camera eye is non-finite");
        }
        let Some(eye_radial_m) = magnitude(eye) else {
            return Err("camera eye is unusable for meteor placement");
        };
        let radius_m = view.game_planet_radius_m;
        if !radius_m.is_finite() || radius_m <= 0.0 {
            return Err("planet radius is unusable for meteor placement");
        }
        let altitude_phys_m = scale.physical_altitude_m(eye_radial_m - radius_m);
        if !altitude_phys_m.is_finite() {
            return Err("surface scale is unusable for meteor placement");
        }
        let eye_up = [
            eye[0] / eye_radial_m,
            eye[1] / eye_radial_m,
            eye[2] / eye_radial_m,
        ];
        let forward = [
            -f64::from(view.view[0][2]),
            -f64::from(view.view[1][2]),
            -f64::from(view.view[2][2]),
        ];
        let radial = forward.iter().zip(eye_up).map(|(a, b)| a * b).sum::<f64>();
        let horizontal = std::array::from_fn(|i| forward[i] - radial * eye_up[i]);
        let mut placement = Rng {
            state: self.seed ^ 0xa1d58b37c4e6920f,
        };
        let heading = if self.automatic {
            // Observer position anchors the local sky, never camera heading,
            // FOV, daylight, clouds or whether this candidate will be visible.
            let axis = if eye_up[2].abs() < 0.9 {
                [0.0, 0.0, 1.0]
            } else {
                [1.0, 0.0, 0.0]
            };
            let east = normalized(cross(eye_up, axis)).ok_or("invalid sky basis")?;
            let north = cross(eye_up, east);
            let (s, c) = (placement.next_f64() * TAU).sin_cos();
            std::array::from_fn(|i| east[i] * c + north[i] * s)
        } else {
            normalized(horizontal).unwrap_or([0.0; 3])
        };
        let spread = if self.automatic {
            0.5 + placement.next_f64() * 7.5
        } else {
            2.0
        };
        let offset = (scale.radial_offset_m(90_000.0) - (eye_radial_m - radius_m)).abs() * spread;
        let up = normalized(std::array::from_fn(|i| eye[i] + heading[i] * offset))
            .ok_or("meteor placement is degenerate")?;
        let axis = if up[2].abs() < 0.9 {
            [0.0, 0.0, 1.0]
        } else {
            [1.0, 0.0, 0.0]
        };
        let Some(first) = normalized(cross(up, axis)) else {
            return Err("meteor basis is degenerate");
        };
        let Some(second) = normalized(cross(up, first)) else {
            return Err("meteor basis is degenerate");
        };
        let mut rng = Rng { state: self.seed };
        let azimuth = rng.next_f64() * TAU;
        let fragmenting = self.preset == crate::client_commands::MeteorPreset::Fragmenting;
        let start_height_m =
            80_000.0 + rng.next_f64() * if fragmenting { 5000.0 } else { 30_000.0 };
        let descent_m = if fragmenting {
            35_000.0
        } else {
            20_000.0 + rng.next_f64() * 15_000.0
        };
        let speed_m_s =
            (self.params.speed_m_s * (0.9 + rng.next_f64() * 0.2)).clamp(11_000.0, 72_000.0);
        if !speed_m_s.is_finite() {
            return Err("meteor speed is finite only after clamping");
        }
        let (sin_azimuth, cos_azimuth) = azimuth.sin_cos();
        let cos_sink = super::presets::entry(self.preset)
            .map_or((descent_m / (speed_m_s * self.duration_s)).min(0.85), |v| {
                v.1
            });
        let sin_sink = (1.0 - cos_sink * cos_sink).sqrt();
        let bearing = [
            first[0] * cos_azimuth + second[0] * sin_azimuth,
            first[1] * cos_azimuth + second[1] * sin_azimuth,
            first[2] * cos_azimuth + second[2] * sin_azimuth,
        ];
        let velocity_m_s = [
            speed_m_s * (bearing[0] * sin_sink - up[0] * cos_sink),
            speed_m_s * (bearing[1] * sin_sink - up[1] * cos_sink),
            speed_m_s * (bearing[2] * sin_sink - up[2] * cos_sink),
        ];
        let start_radius_m =
            radius_m + super::presets::entry(self.preset).map_or(start_height_m, |v| v.0);
        if !start_radius_m.is_finite() || start_radius_m.abs() > POSITION_ABS_M_MAX {
            return Err("meteor start radius leaves the contract bounds");
        }
        let start_phys_m = [
            up[0] * start_radius_m,
            up[1] * start_radius_m,
            up[2] * start_radius_m,
        ];
        self.basis = Some(EventBasis {
            start_phys_m,
            velocity_m_s,
        });
        let history = self.evolution.as_mut().ok_or("missing pooled history")?;
        if !history.initialize(
            self.basis.ok_or("missing flight basis")?,
            radius_m,
            self.params.mass_kg,
            self.seed,
            self.preset,
        ) {
            return Err("invalid physical history");
        }
        self.duration_s = history.duration_s() + self.trains.profile.ttl_s();
        Ok(())
    }
}

/// `None` for zero vectors and non-finite components.
fn normalized(vector: [f64; 3]) -> Option<[f64; 3]> {
    magnitude(vector).map(|value| [vector[0] / value, vector[1] / value, vector[2] / value])
}

/// World-right-handed exterior product of two vectors.
fn cross(first: [f64; 3], second: [f64; 3]) -> [f64; 3] {
    [
        first[1] * second[2] - first[2] * second[1],
        first[2] * second[0] - first[0] * second[2],
        first[0] * second[1] - first[1] * second[0],
    ]
}

/// Deterministic splitmix64 step; the only randomness source of this runtime.
pub(super) fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_8CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Fixed-width deterministic stream over one event seed.
struct Rng {
    state: u64,
}

impl Rng {
    /// Uniform value in `[0, 1)`; 53 mantissa bits are consumed per call.
    fn next_f64(&mut self) -> f64 {
        let raw = splitmix64(&mut self.state) >> 11;
        raw as f64 / (1u64 << 53) as f64
    }
}
