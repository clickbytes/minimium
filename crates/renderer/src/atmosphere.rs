//! Fixed clear-air optical constants used by the Earth atmosphere renderer.

/// Spherical Earth atmosphere model inputs for render-time LUT construction.
#[derive(Clone, Copy)]
pub struct Optics {
    pub radius_m: f64,
    pub atmospheric_top_m: f64,
    pub rayleigh_scale_height_m: f64,
    pub aerosol_scale_height_m: f64,
    pub rayleigh_rgb_m_inv: [f64; 3],
    pub mie_m_inv: f64,
    pub ozone_rgb_m_inv: [f64; 3],
    pub mie_anisotropy: f64,
    pub ozone_peak_height_m: f64,
    pub ozone_half_width_m: f64,
}

/// Earth clear-air profile. This is a renderer model, separate from weather.
pub const EARTH: Optics = Optics {
    radius_m: 6_371_000.0,
    atmospheric_top_m: 100_000.0,
    rayleigh_scale_height_m: 8000.0,
    aerosol_scale_height_m: 1200.0,
    rayleigh_rgb_m_inv: [5.802e-6, 13.558e-6, 33.1e-6],
    mie_m_inv: 4.44e-6,
    ozone_rgb_m_inv: [0.650e-6, 1.881e-6, 0.085e-6],
    mie_anisotropy: 0.76,
    ozone_peak_height_m: 25_000.0,
    ozone_half_width_m: 15_000.0,
};
