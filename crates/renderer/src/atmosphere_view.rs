//! Pure camera/optical-scale preparation, independent of the player observer.
use crate::{AtmosphereUniform, RenderViewFrame};
use glam::DVec3;

/// Catalog and diffuse galactic contrast before cloud/air transmission and camera exposure.
pub fn celestial_visibility(
    sky: &crate::gpu_frame::EnvironmentUniform,
    air: &AtmosphereUniform,
) -> (f32, f32) {
    if sky.flags[2] == 0
        || sky.flags[0] == minimium_environment::EnvironmentProfile::Interior as u32
    {
        return (0.0, 0.0);
    }
    if sky.flags[1] == crate::EnvironmentViewKind::Overview as u32 {
        return (1.0, 1.0);
    }
    let above_air = stellar_altitude_weight(air);
    let surface = sky.sky_horizon_rgb_star_visibility[3];
    let stars = surface + (1.0 - surface) * above_air;
    if air.config[3] > 0.5 {
        // The physical sky adds the actual scattered lunar radiance over the stars.
        return (stars, stars);
    }
    // Approximate the lunar sky glow on the analytic path using the incident
    // light on a horizontal surface relative to the overhead full Moon.
    let luma = |[r, g, b, _]: [f32; 4]| crate::render_frame::luminance([r, g, b]);
    let global = luma(sky.global_moon_rgb);
    let transmission = if global > 0.0 {
        luma(sky.local_moon_rgb_visibility) / global
    } else {
        0.0
    };
    let moon = glam::Vec4::from_array(sky.moon_apparent_world).truncate();
    let up = glam::Vec4::from_array(sky.observer_up_horizon_cos).truncate();
    let lunar_glow =
        sky.moon_disk_radiance[3] * transmission * moon.dot(up).max(0.0) * (1.0 - above_air);
    (stars, stars / (1.0 + 8.0 * lunar_glow))
}

pub(crate) fn stellar_altitude_weight(air: &AtmosphereUniform) -> f32 {
    if air.config[0] <= 0.0 {
        return 0.0;
    }
    let t = ((air.up_height[3] - 12_000.0) / 68_000.0).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[derive(Clone)]
pub struct WeatherEndpoints {
    pub utc: f64,
    pub center: DVec3,
    pub values: [[f32; 4]; 2],
    pub basis: [[f32; 4]; 3],
    pub has_fog: bool,
}
impl WeatherEndpoints {
    pub fn from_grid(grid: &crate::surface_cover::CoverGrid) -> Self {
        let axis = |v| glam::Vec4::from_array(v).truncate().as_dvec3();
        Self {
            utc: grid.start_utc,
            center: (axis(grid.center)
                + axis(grid.east) * f64::from(grid.east[3])
                + axis(grid.north) * f64::from(grid.north[3]))
            .normalize(),
            values: grid.atmosphere_weather,
            basis: [grid.center, grid.east, grid.north],
            has_fog: grid
                .fog_pixels
                .iter()
                .any(|p| p[0].to_f32() + p[3].to_f32() > 1e-8),
        }
    }
}
pub fn prepare(
    view: RenderViewFrame,
    radius: f64,
    length_scale: f64,
    standard: bool,
    utc: f64,
    weather: Option<&WeatherEndpoints>,
    debug: u32,
    enabled: bool,
) -> AtmosphereUniform {
    let p = crate::atmosphere::EARTH;
    let scale = 1.0 / length_scale.clamp(0.001, 1.0);
    let eye = DVec3::from_array(view.camera_eye_world_m);
    let up = eye.normalize_or_zero();
    let h = ((eye.length() - radius) * scale).max(-12000.0);
    let optical_radius = (radius * scale).max(1.0);
    let relative = eye - DVec3::from_array(view.render_origin_m);
    let mut basis = [[0.0; 4]; 3];
    let mut fog_time = [0.0; 4];
    if let Some(w) = weather {
        basis = w.basis;
        let phase = ((utc - w.utc) / 3600.0).clamp(0.0, 1.0) as f32;
        let age = ((utc - w.utc - 3600.0) / 3600.0).clamp(0.0, 1.0) as f32;
        fog_time = [
            phase,
            if utc >= w.utc {
                1.0 - age * age * (3.0 - 2.0 * age)
            } else {
                0.0
            },
            f32::from(w.has_fog),
            0.0,
        ];
    }
    AtmosphereUniform {
        radii: [
            optical_radius as f32,
            p.atmospheric_top_m as f32,
            p.rayleigh_scale_height_m as f32,
            p.aerosol_scale_height_m as f32,
        ],
        rayleigh: [
            p.rayleigh_rgb_m_inv[0] as f32,
            p.rayleigh_rgb_m_inv[1] as f32,
            p.rayleigh_rgb_m_inv[2] as f32,
            0.0,
        ],
        mie: [
            p.mie_m_inv as f32,
            p.mie_anisotropy as f32,
            p.ozone_half_width_m as f32,
            0.0,
        ],
        ozone: [
            p.ozone_rgb_m_inv[0] as f32,
            p.ozone_rgb_m_inv[1] as f32,
            p.ozone_rgb_m_inv[2] as f32,
            p.ozone_peak_height_m as f32,
        ],
        config: [
            radius as f32,
            scale as f32,
            f32::from(standard),
            f32::from(enabled),
        ],
        up_height: up.as_vec3().extend(h as f32).to_array(),
        eye_relative: relative
            .as_vec3()
            .extend(
                (300000.0_f64.max(2.0 * (2.0 * optical_radius * h.max(0.0)).sqrt())).min(3e7)
                    as f32,
            )
            .to_array(),
        weather: [0.0, 100.0, 0.0, debug as f32],
        fog_center: basis[0],
        fog_east: basis[1],
        fog_north: basis[2],
        fog_time,
        relief_tile: [0.0; 4],
        inverse_view_projection: glam::Mat4::from_cols_array_2d(&view.view_projection)
            .inverse()
            .to_cols_array_2d(),
    }
}
