//! Deterministic liquid condensation in the surface boundary layer.
use super::{climate, SeasonalProfile, TerrainSampler, WeatherModel, EARTH_RADIUS};
use glam::DVec3;

/// Near-surface extinction and its e-folding height above the displayed ground.
/// Heights and attenuation use the same physical metre as the atmosphere.
#[derive(Clone, Copy, Debug)]
pub struct GroundFogSample {
    pub extinction_m_inv: f32,
    pub scale_height_m: f32,
    pub ground_height_m: f32,
    pub haze_m_inv: f32,
}

impl<T: TerrainSampler> WeatherModel<T> {
    /// One terrain preparation for both UTC endpoints of a column.
    pub fn ground_fog_endpoints(
        &self,
        direction: DVec3,
        endpoints_utc: [f64; 2],
    ) -> Result<[GroundFogSample; 2], T::Error> {
        let direction = direction.normalize();
        let site = climate::site(self, direction)?;
        let ground = site.ground / self.elevation_scale;
        let ground = if site.ocean { ground.max(0.0) } else { ground };
        let east = direction.any_orthonormal_vector();
        let north = direction.cross(east);
        // Opposing samples cancel a constant slope. A trough can be open along
        // its length and still collect cold air across the valley.
        let mut depression_m = 0.0_f64;
        if !site.ocean {
            for axis in [east, north] {
                let offset = axis * (2_000.0 / EARTH_RADIUS);
                let upper = self.atmospheric_ground_height_m(direction + offset)?;
                let lower = self.atmospheric_ground_height_m(direction - offset)?;
                depression_m = depression_m.max((lower + upper) * 0.5 - ground);
            }
        }
        let pooling = smooth(depression_m / 80.0);
        Ok(endpoints_utc.map(|utc| {
            let (t, moisture, weather) = climate::climate_at(self, direction, utc, &site);
            let cooling = night_cooling(direction, utc, self.profile, weather);
            let spread = dew_point_spread(t, moisture);
            let air_cooling = cooling * (if site.ocean { 1.7 } else { 3.8 + pooling * 3.0 });
            // A smooth transition represents unresolved variations within a column.
            let condensed =
                smooth((air_cooling - spread + 1.0) / 3.0) * smooth((t - air_cooling + 2.0) / 3.0);
            GroundFogSample {
                extinction_m_inv: (0.014 * condensed) as f32,
                scale_height_m: (6.0 + condensed * (26.0 + pooling * 70.0)) as f32,
                ground_height_m: ground as f32,
                haze_m_inv: (1e-5 * smooth((moisture - 0.65) / 0.30)) as f32,
            }
        }))
    }
}

/// Surface film has a longer memory than the airborne droplets. Replay the
/// last six world hours at fixed steps, independent of the observer's history.
pub(super) fn retained_dew<T: TerrainSampler>(
    model: &WeatherModel<T>,
    direction: DVec3,
    utc: f64,
    site: &climate::Site,
) -> f32 {
    if site.ocean {
        return 0.0;
    }
    let mut film = 0.0;
    for hours_ago in (0..6).rev() {
        let time = utc - f64::from(hours_ago) * 3600.0;
        let (t, moisture, weather) = climate::climate_at(model, direction, time, site);
        let cooling = night_cooling(direction, time, model.profile, weather);
        let surface_cooling = 6.5 * cooling;
        let deposition = smooth((surface_cooling - dew_point_spread(t, moisture)) / 2.5)
            * smooth((t - surface_cooling + 1.0) / 2.0);
        // Sunlit hours dry the film, including when saturation ceases at dawn.
        let drying = 0.12 + 0.54 * (1.0 - cooling);
        film = (film * (1.0 - drying) + deposition * 0.17).min(0.45);
    }
    film as f32
}

fn dew_point_spread(temperature_c: f64, moisture: f64) -> f64 {
    // Weather moisture is a supply index. Calibrate it to a reference RH
    // before applying the Magnus relationship for liquid water.
    let rh = 0.40 + 0.60 * moisture.clamp(0.0, 1.0);
    let t = temperature_c.clamp(-45.0, 60.0);
    let gamma = rh.ln() + 17.62 * t / (243.12 + t);
    t - 243.12 * gamma / (17.62 - gamma)
}

fn night_cooling(direction: DVec3, utc: f64, profile: SeasonalProfile, weather: f64) -> f64 {
    let declination = if profile == SeasonalProfile::Seasonal {
        23.44_f64.to_radians() * crate::climate::seasonality(DVec3::Y, utc)
    } else {
        0.0
    };
    // A two-hour thermal lag carries nighttime cooling through sunrise.
    let phase = (utc - 7200.0).rem_euclid(86400.0) * std::f64::consts::TAU / 86400.0;
    let solar_height = direction.y * declination.sin()
        - (direction.x * phase.cos() - direction.z * phase.sin()) * declination.cos();
    let clear_sky = 1.0 - 0.45 * smooth(weather);
    smooth((0.25 - solar_height) / 0.65) * clear_sky
}

fn smooth(t: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}
