//! Bounded UTC trajectory reconstruction. Cached maps contain displacements, not positions.
use super::wind::{self, WindState, EARTH_RADIUS};
use glam::{DVec3, Vec3};
use std::{cell::RefCell, collections::VecDeque, sync::Arc};

const WIDTH: usize = 48;
const HEIGHT: usize = WIDTH / 2;
const HEIGHTS: [f64; 4] = [0.0, 3000.0, 8000.0, 16000.0];
pub(crate) const COHORT_SECONDS: f64 = 10800.0;

struct Map(Vec<Vec3>);

impl Map {
    fn build(from: f64, to: f64, height: f64, seed: u128) -> Self {
        // Fixed UTC endpoints and bounded RK2 steps; no client frame history.
        let mut time = from;
        let mut states = Vec::new();
        while (to - time).abs() > 0.0001 {
            let next = if to < time {
                ((time / 600.0).ceil() - 1.0) * 600.0
            } else {
                ((time / 600.0).floor() + 1.0) * 600.0
            };
            let next = if to < time {
                next.max(to)
            } else {
                next.min(to)
            };
            let dt = next - time;
            states.push((
                dt,
                WindState::new(time, seed),
                WindState::new(time + dt * 0.5, seed),
            ));
            time = next;
        }
        Self(
            (0..WIDTH * (HEIGHT + 1))
                .map(|i| {
                    let (lat, lon) = super::grid::coordinates(WIDTH, i % WIDTH, i / WIDTH);
                    let start = DVec3::new(lat.cos() * lon.cos(), lat.sin(), lat.cos() * lon.sin());
                    let mut up = start;
                    for &(dt, first, middle) in &states {
                        let midpoint = wind::displace(up, first.velocity(up, height) * (dt * 0.5));
                        up = wind::displace(up, middle.velocity(midpoint, height) * dt);
                    }
                    (up - start).as_vec3()
                })
                .collect(),
        )
    }

    fn sample(&self, up: DVec3) -> DVec3 {
        let x = (up.z.atan2(up.x) / std::f64::consts::TAU + 0.5).rem_euclid(1.0) * WIDTH as f64;
        let y = (0.5 - up.y.clamp(-1.0, 1.0).asin() / std::f64::consts::PI) * HEIGHT as f64;
        let ix = x.floor() as usize % WIDTH;
        let iy = (y.floor() as usize).min(HEIGHT);
        let read = |x: usize, y: usize| self.0[y.min(HEIGHT) * WIDTH + x % WIDTH];
        let a = read(ix, iy).lerp(read(ix + 1, iy), x.fract() as f32);
        let b = read(ix, iy + 1).lerp(read(ix + 1, iy + 1), x.fract() as f32);
        (up + a.lerp(b, y.fract() as f32).as_dvec3()).normalize()
    }
}

pub(crate) struct Cohort {
    pub(crate) id: i64,
    pub(crate) age: f64,
    maps: [Map; 4],
}

pub(crate) struct FlowFrame {
    pub(crate) seconds: f64,
    pub(crate) seed: u128,
    pub(crate) wind: WindState,
    pub(crate) cohorts: [Cohort; 3],
}

impl FlowFrame {
    fn new(seconds: f64, seed: u128) -> Self {
        let epoch = (seconds / COHORT_SECONDS).floor() as i64;
        Self {
            seconds,
            seed,
            wind: WindState::new(seconds, seed),
            cohorts: std::array::from_fn(|i| {
                let id = epoch - i as i64;
                let birth = id as f64 * COHORT_SECONDS;
                Cohort {
                    id,
                    age: seconds - birth,
                    maps: HEIGHTS.map(|height| Map::build(seconds, birth, height, seed)),
                }
            }),
        }
    }

    pub(crate) fn origin(&self, up: DVec3, height: f64, cohort: usize) -> DVec3 {
        interpolate(&self.cohorts[cohort].maps, up, height)
    }

    /// Integrate local land/sea circulation over a short physical residence time.
    /// Terrain forcing is local; its history is reconstructed from the solar cycle.
    pub(crate) fn coastal_displacement(
        &self,
        up: DVec3,
        height_agl: f64,
        shore: [f64; 2],
        duration: f64,
    ) -> DVec3 {
        if shore[0].abs() + shore[1].abs() < 0.02 || height_agl > 9000.0 {
            return DVec3::ZERO;
        }
        let duration = duration.clamp(0.0, 1800.0);
        let dt = duration / 6.0;
        let mut displacement = DVec3::ZERO;
        for i in 0..6 {
            let state = WindState::new(self.seconds - (i as f64 + 0.5) * dt, self.seed);
            displacement += state.breeze(up, shore, height_agl) * dt;
        }
        displacement
    }

    pub(crate) fn velocity(&self, up: DVec3, height: f64, ground: f64, shore: [f64; 2]) -> DVec3 {
        self.wind.velocity(up, height) + self.wind.breeze(up, shore, height - ground)
    }
}

fn interpolate(maps: &[Map; 4], up: DVec3, height: f64) -> DVec3 {
    let lower = HEIGHTS
        .partition_point(|&value| value <= height)
        .saturating_sub(1)
        .min(2);
    let t = ((height - HEIGHTS[lower]) / (HEIGHTS[lower + 1] - HEIGHTS[lower])).clamp(0.0, 1.0);
    maps[lower]
        .sample(up)
        .lerp(maps[lower + 1].sample(up), t)
        .normalize()
}

thread_local! {
    // Worker-local: no global lock, bounded memory, no camera-dependent simulation state.
    static FRAMES: RefCell<VecDeque<Arc<FlowFrame>>> = const { RefCell::new(VecDeque::new()) };
}

pub(crate) fn frame(seconds: f64, seed: u128) -> Arc<FlowFrame> {
    let seconds = (seconds * 1000.0).round() / 1000.0;
    FRAMES.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some(index) = cache
            .iter()
            .position(|entry| entry.seconds == seconds && entry.seed == seed)
        {
            let frame = cache.remove(index).unwrap();
            cache.push_front(frame.clone());
            return frame;
        }
        let frame = Arc::new(FlowFrame::new(seconds, seed));
        if cache.len() == 8 {
            cache.pop_back();
        }
        cache.push_front(frame.clone());
        frame
    })
}

/// Bound top displacement independently of the base of the cloud.
pub(crate) fn lean(
    up: DVec3,
    base_wind: DVec3,
    top_wind: DVec3,
    thickness: f64,
    convection: f64,
) -> DVec3 {
    let rise_time = (thickness / (1.5 + convection * 8.0)).clamp(0.0, 600.0);
    wind::tangent((top_wind - base_wind) * rise_time * convection, up)
        .clamp_length_max((thickness * 0.7).min(EARTH_RADIUS * 0.0006))
}
