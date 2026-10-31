//! Uniform upload is kept separate to maintain the project's 500-line policy.
use super::{WeatherFrame, WeatherGpu, WeatherUniform};

impl WeatherGpu {
    pub(in crate::renderer) fn upload(&mut self, queue: &wgpu::Queue, frame: WeatherFrame) {
        let WeatherFrame {
            rain_ceiling_m,
            readable_globe,
            sample,
            blend,
            radius,
            origin,
            utc_ms,
            lightning,
            standard_quality,
            shelter_mask,
            region,
            overview,
            seed,
        } = frame;
        let (bolt, mut event) = lightning.map_or(([0.0; 4], [0.0; 4]), |flash| {
            (
                [
                    flash.direction[0],
                    flash.direction[1],
                    flash.direction[2],
                    flash.intensity,
                ],
                [(flash.id as u32) as f32 / u32::MAX as f32, 0.0, 0.0, 0.0],
            )
        });
        event[1] = self.instances.morph;
        event[2] = rain_ceiling_m;
        let p = self.atmosphere_profile;
        let value = WeatherUniform {
            atmosphere_radii: [
                p.radius_m as f32, p.atmospheric_top_m as f32,
                p.rayleigh_scale_height_m as f32, p.aerosol_scale_height_m as f32,
            ],
            atmosphere_rayleigh: [
                p.rayleigh_rgb_m_inv[0] as f32, p.rayleigh_rgb_m_inv[1] as f32,
                p.rayleigh_rgb_m_inv[2] as f32, 0.0,
            ],
            atmosphere_mie: [
                p.mie_m_inv as f32, p.mie_anisotropy as f32,
                p.ozone_half_width_m as f32, 0.0,
            ],
            atmosphere_ozone: [
                p.ozone_rgb_m_inv[0] as f32, p.ozone_rgb_m_inv[1] as f32,
                p.ozone_rgb_m_inv[2] as f32, p.ozone_peak_height_m as f32,
            ],
            presentation: [
                f32::from(readable_globe && overview),
                0.80,
                0.018,
                1.0 / self.capabilities.optical_scale(),
            ],
            cloud_region: self.instances.region,
            cover_rain: [sample.low, sample.middle, sample.high, sample.rain_mm_h],
            wind: [
                sample.wind_east_m_s,
                sample.wind_north_m_s,
                (radius / 6_371_000.0).max(0.0001),
                sample.temperature_c,
            ],
            blend: [
                blend,
                radius,
                utc_ms.unwrap_or(0).rem_euclid(60_000) as f32 / 1000.0,
                f32::from(standard_quality),
            ],
            origin: [origin[0], origin[1], origin[2], 0.0],
            lightning: bolt,
            event,
            shelter: shelter_mask.map(f32::from),
            region,
            animation: [
                utc_ms.unwrap_or(0).rem_euclid(21_600_000) as f32 / 1000.0,
                utc_ms
                    .unwrap_or(0)
                    .div_euclid(21_600_000)
                    .rem_euclid(16_777_216) as f32,
                f32::from(overview),
                ((seed as u64 ^ (seed >> 64) as u64) as u32) as f32 / u32::MAX as f32,
            ],
        };
        queue.write_buffer(&self.uniform, 0, bytemuck::bytes_of(&value));
        self.rain_mm_h = sample.rain_mm_h;
        self.rain_particles = ((sample.rain_mm_h / 32.0).sqrt() * 1024.0)
            .round()
            .clamp(0.0, 1024.0) as u32;
        self.bolt_active = lightning.is_some();
    }


}
