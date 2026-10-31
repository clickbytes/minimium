//! Physical planet inputs. Optics are data, not renderer-specific constants.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Condensable {
    None,
    Water,
}

/// Validated dry-gas molar fractions. Optical coefficients stay explicit.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GasComposition {
    pub nitrogen: f64,
    pub oxygen: f64,
    pub argon: f64,
    pub carbon_dioxide: f64,
    pub other: f64,
}

impl GasComposition {
    pub const fn earth() -> Self {
        Self { nitrogen: 0.78084, oxygen: 0.20946, argon: 0.00934,
            carbon_dioxide: 0.00036, other: 0.0 }
    }
    pub fn validate(self) -> Result<(), &'static str> {
        let x = [self.nitrogen, self.oxygen, self.argon,
            self.carbon_dioxide, self.other];
        if x.iter().any(|v| !v.is_finite() || *v < 0.0 || *v > 1.0)
            || (x.iter().sum::<f64>() - 1.0).abs() > 0.001 {
            return Err("gas fractions must be nonnegative and sum to one");
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AtmosphereProfile {
    pub radius_m: f64,
    pub composition: GasComposition,
    pub gravity_m_s2: f64,
    pub rotation_rad_s: f64,
    pub orbital_period_s: f64,
    pub obliquity_rad: f64,
    pub stellar_flux_w_m2: f64,
    pub surface_pressure_pa: f64,
    pub surface_temperature_k: f64,
    pub heat_capacity_j_kg_k: f64,
    pub atmospheric_top_m: f64,
    pub rayleigh_scale_height_m: f64,
    pub aerosol_scale_height_m: f64,
    pub rayleigh_rgb_m_inv: [f64; 3],
    pub mie_m_inv: f64,
    pub ozone_rgb_m_inv: [f64; 3],
    pub mie_anisotropy: f64,
    pub ozone_peak_height_m: f64,
    pub ozone_half_width_m: f64,
    pub condensable: Condensable,
}

impl AtmosphereProfile {
    pub const fn earth() -> Self {
        Self {
            radius_m: 6_371_000.0,
            composition: GasComposition::earth(),
            gravity_m_s2: 9.80665,
            rotation_rad_s: 7.2921159e-5,
            orbital_period_s: 365.2425 * 86_400.0,
            obliquity_rad: 0.4090928,
            stellar_flux_w_m2: 1361.0,
            surface_pressure_pa: 101_325.0,
            surface_temperature_k: 288.15,
            heat_capacity_j_kg_k: 1004.0,
            atmospheric_top_m: 100_000.0,
            rayleigh_scale_height_m: 8000.0,
            aerosol_scale_height_m: 1200.0,
            rayleigh_rgb_m_inv: [5.802e-6, 13.558e-6, 33.1e-6],
            mie_m_inv: 4.44e-6,
            ozone_rgb_m_inv: [0.650e-6, 1.881e-6, 0.085e-6],
            mie_anisotropy: 0.76,
            ozone_peak_height_m: 25_000.0,
            ozone_half_width_m: 15_000.0,
            condensable: Condensable::Water,
        }
    }

    pub fn validate(self) -> Result<(), &'static str> {
        self.composition.validate()?;
        for value in [
            self.radius_m, self.gravity_m_s2, self.orbital_period_s,
            self.stellar_flux_w_m2, self.surface_pressure_pa,
            self.surface_temperature_k, self.heat_capacity_j_kg_k,
            self.atmospheric_top_m, self.rayleigh_scale_height_m,
            self.aerosol_scale_height_m, self.ozone_half_width_m,
        ] {
            if !value.is_finite() || value <= 0.0 {
                return Err("positive finite atmosphere parameter required");
            }
        }
        if !self.rotation_rad_s.is_finite() || self.rotation_rad_s.abs() > 0.01
            || !self.obliquity_rad.is_finite()
            || self.obliquity_rad.abs() > std::f64::consts::FRAC_PI_2
        {
            return Err("invalid planetary rotation or axial tilt");
        }
        if self.radius_m < 1000.0 || self.atmospheric_top_m > self.radius_m * 0.5
            || self.surface_temperature_k < 50.0 || self.surface_temperature_k > 1000.0
        {
            return Err("planet atmosphere scale is unsupported");
        }
        if !self.mie_anisotropy.is_finite() || self.mie_anisotropy.abs() >= 0.99
            || !self.ozone_peak_height_m.is_finite()
            || self.ozone_peak_height_m < 0.0
            || self.ozone_peak_height_m > self.atmospheric_top_m {
            return Err("invalid aerosol phase or ozone layer geometry");
        }
        for value in self.rayleigh_rgb_m_inv.into_iter()
            .chain(self.ozone_rgb_m_inv)
            .chain([self.mie_m_inv])
        {
            if !value.is_finite() || value < 0.0 || value > 1.0 {
                return Err("atmospheric extinction must be finite and nonnegative");
            }
        }
        Ok(())
    }
}
