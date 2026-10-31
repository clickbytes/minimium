//! Continuous physical-metre velocity fields, independent of rendering and frame rate.
use glam::DVec3;

pub(crate) const EARTH_RADIUS: f64 = 6_371_000.0;

#[derive(Clone, Copy)]
pub(crate) struct WindState {
    axes: [DVec3; 3],
    strength: [f64; 3],
    upper_axis: DVec3,
    seconds: f64,
}

impl WindState {
    pub(crate) fn new(seconds: f64, seed: u128) -> Self {
        let key = (seed as u64 ^ (seed >> 64) as u64) as u32;
        let phase = std::array::from_fn::<_, 3, _>(|i| {
            (seconds / [310000.0, 470000.0, 730000.0][i]
                + hash(key.wrapping_add(i as u32 * 719)) * std::f64::consts::TAU)
                .rem_euclid(std::f64::consts::TAU)
        });
        Self {
            axes: std::array::from_fn(|i| {
                DVec3::new(phase[i].cos(), [0.45, -0.55, 0.18][i], phase[i].sin()).normalize()
            }),
            strength: [9.0 + phase[1].sin() * 3.0, -8.0 + phase[2].sin() * 2.0, 5.0],
            upper_axis: DVec3::new(phase[0].sin() * 0.2, 1.0, phase[1].cos() * 0.15).normalize(),
            seconds,
        }
    }

    pub(crate) fn velocity(self, up: DVec3, height: f64) -> DVec3 {
        let upper = smooth(0.0, 14000.0, height);
        let belt = ((up.y.abs() - 0.52) * 7.0).tanh();
        let mut velocity = DVec3::Y.cross(up) * belt * (7.0 + upper * 17.0);
        for (i, axis) in self.axes.into_iter().enumerate() {
            let distance = 1.0 - axis.dot(up);
            let influence = 0.25 + 1.0 / (1.0 + distance * distance * 12.0);
            let circulation = axis.cross(up) * self.strength[i] * influence;
            // Upper flow veers continuously; near-surface convergence weakens aloft.
            velocity += circulation * (1.0 + upper * 0.8)
                + tangent(axis, up) * self.strength[i] * influence * (0.16 - upper * 0.24);
        }
        velocity += self.upper_axis.cross(up) * upper * 6.0;
        // Regional circulation stays finite at the poles, even when zonal flow vanishes.
        tangent(velocity, up).clamp_length_max(45.0)
    }

    pub(crate) fn solar(self, up: DVec3) -> f64 {
        let days = self.seconds / 86400.0;
        let declination = 0.409 * ((days - 80.0) / 365.2425 * std::f64::consts::TAU).sin();
        let angle =
            (days.rem_euclid(1.0) * std::f64::consts::TAU).rem_euclid(std::f64::consts::TAU);
        up.y * declination.sin() - declination.cos() * (up.x * angle.cos() - up.z * angle.sin())
    }

    pub(crate) fn breeze(self, up: DVec3, shore: [f64; 2], height_agl: f64) -> DVec3 {
        let (east, north) = basis(up);
        let speed = self.velocity(up, 0.0).length();
        let strength = (self.solar(up) * 3.5).clamp(-1.5, 3.5)
            * (1.0 - speed / 18.0).clamp(0.0, 1.0)
            * (-height_agl.max(0.0) / 1800.0).exp();
        (east * shore[0] + north * shore[1]) * strength
    }
}

pub(crate) fn basis(up: DVec3) -> (DVec3, DVec3) {
    let east = DVec3::new(-up.z, 0.0, up.x)
        .try_normalize()
        .unwrap_or(DVec3::Z);
    (east, east.cross(up))
}

pub(crate) fn tangent(vector: DVec3, up: DVec3) -> DVec3 {
    vector - up * vector.dot(up)
}

pub(crate) fn displace(up: DVec3, displacement_m: DVec3) -> DVec3 {
    let tangent = tangent(displacement_m, up);
    let length = tangent.length();
    if length < 1e-9 {
        return up;
    }
    let arc = length / EARTH_RADIUS;
    up * arc.cos() + tangent / length * arc.sin()
}

pub(crate) fn smooth(a: f64, b: f64, value: f64) -> f64 {
    let t = ((value - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

pub(super) fn hash(mut value: u32) -> f64 {
    value = (value ^ (value >> 16)).wrapping_mul(0x7feb352d);
    value = (value ^ (value >> 15)).wrapping_mul(0x846ca68b);
    f64::from((value ^ (value >> 16)) >> 8) / 16777216.0
}
