//! CPU grid published by a host worker and uploaded without history work in a frame.
use glam::DVec3;
use minimium_environment::weather::{TerrainSampler, WeatherModel};

pub const EDGE: u32 = 9;
pub const EXTENT: f64 = 0.01;
const STEP: f64 = EXTENT * 2.0 / (EDGE - 1) as f64;

/// Exact addresses of the cube-face fog tile and the latitude/longitude cover tile.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct CoverAddress {
    axis: usize,
    sign: i32,
    u: i32,
    v: i32,
    latitude: i32,
    longitude: i32,
}

impl CoverAddress {
    pub fn at(observer: DVec3) -> Self {
        let (axis, sign) = crate::fog_relief::face_axes(observer);
        let depth = observer[axis].abs().max(f64::EPSILON);
        Self {
            axis,
            sign: sign as i32,
            u: (observer[(axis + 1) % 3] / depth / STEP).round() as i32,
            v: (observer[(axis + 2) % 3] / depth / STEP).round() as i32,
            latitude: (observer.y.clamp(-1.0, 1.0).asin() / STEP).floor() as i32,
            longitude: (observer.z.atan2(observer.x) / STEP).floor() as i32,
        }
    }
}

pub struct CoverGrid {
    pub address: CoverAddress,
    pub start_utc: f64,
    pub region: [f32; 4],
    pub atlas: Option<std::sync::Arc<crate::surface_atlas::SurfaceAtlas>>,
    pub center: [f32; 4],
    pub east: [f32; 4],
    pub north: [f32; 4],
    pub pixels: Vec<[half::f16; 4]>,
    pub atmosphere_weather: [[f32; 4]; 2],
    pub fog_pixels: Vec<[half::f16; 4]>,
}

impl CoverGrid {
    pub fn build<T: TerrainSampler>(
        model: &WeatherModel<T>,
        observer: DVec3,
        utc: f64,
    ) -> Result<Self, T::Error> {
        let address = CoverAddress::at(observer);
        let axis = address.axis;
        let axes = [DVec3::X, DVec3::Y, DVec3::Z];
        let face = axes[axis] * f64::from(address.sign);
        let east = axes[(axis + 1) % 3];
        let north = axes[(axis + 2) % 3];
        let start_utc = (utc / 3600.0).floor() * 3600.0;
        // Fixed angular lattice: overlapping windows evaluate identical sites.
        let step = STEP;
        let u_center = f64::from(address.u) * step;
        let v_center = f64::from(address.v) * step;
        let start_lat = f64::from(address.latitude) * step - EXTENT;
        let start_lon = f64::from(address.longitude) * step - EXTENT;
        let cells = (EDGE * EDGE) as usize;
        let mut pixels = vec![[half::f16::ZERO; 4]; cells * 4];
        let mut atmosphere_weather = [[0.0; 4]; 2];
        let mut fog_pixels = vec![[half::f16::ZERO; 4]; cells * 2];
        let times = [start_utc, start_utc + 3600.0];
        for y in 0..EDGE {
            for x in 0..EDGE {
                let u = (f64::from(x) / f64::from(EDGE - 1) * 2.0 - 1.0) * EXTENT;
                let v = (f64::from(y) / f64::from(EDGE - 1) * 2.0 - 1.0) * EXTENT;
                let direction = (face + east * (u_center + u) + north * (v_center + v)).normalize();
                let endpoints = model.ground_fog_endpoints(direction, times)?;
                let site = crate::surface_atlas::direction(
                    start_lat + f64::from(y) * step,
                    start_lon + f64::from(x) * step,
                );
                let index = (y * EDGE + x) as usize;
                for (hour, fog) in endpoints.into_iter().enumerate() {
                    let time = times[hour];
                    fog_pixels[hour * cells + index] = [
                        fog.extinction_m_inv,
                        fog.scale_height_m,
                        fog.ground_height_m,
                        fog.haze_m_inv,
                    ]
                    .map(half::f16::from_f32);
                    if x == EDGE / 2 && y == EDGE / 2 {
                        let state = model.surface_state(direction, time)?;
                        atmosphere_weather[hour] = [
                            state.climate.moisture as f32,
                            state.climate.temperature_c as f32,
                            state.cover.wetness,
                            fog.ground_height_m,
                        ];
                    }
                    let [cover, climate] = crate::surface_atlas::sample(model, site, time)?;
                    pixels[hour * 2 * cells + index] = cover;
                    pixels[(hour * 2 + 1) * cells + index] = climate;
                }
            }
        }
        Ok(Self {
            address,
            start_utc,
            region: [
                start_lon as f32,
                start_lat as f32,
                step as f32,
                (EXTENT * 2.0) as f32,
            ],
            atlas: None,
            center: face.as_vec3().extend(EXTENT as f32).to_array(),
            east: east.as_vec3().extend(u_center as f32).to_array(),
            north: north.as_vec3().extend(v_center as f32).to_array(),
            pixels,
            atmosphere_weather,
            fog_pixels,
        })
    }
}
