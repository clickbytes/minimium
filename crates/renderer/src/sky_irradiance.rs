//! Cosine-weighted upper-hemisphere irradiance for unit collimated source flux.
//! Each sky ray integrates single scattering between the receiver and the upper
//! atmosphere. The incident leg samples the same spherical optical-depth table
//! as direct illumination. No camera, Moon phase, weather or exposure enters here.
use crate::atmosphere::Optics;
use glam::DVec3;
use std::f64::consts::{PI, TAU};

pub(super) const WIDTH: u32 = 65;
pub(super) const HEIGHT: u32 = 17;
const ZENITH: usize = 6;
const AZIMUTH: usize = 12;
const SEGMENTS: usize = 20;

struct Segment {
    up: DVec3,
    horizon: f64,
    optical_row: f64,
    rayleigh: DVec3,
    aerosol: DVec3,
}

fn view_path(p: Optics, height: f64, direction: DVec3) -> Vec<Segment> {
    let r = p.radius_m + height;
    let b = r * direction.z;
    let c = (p.atmospheric_top_m - height) * (2.0 * p.radius_m + p.atmospheric_top_m + height);
    let distance = c / ((b * b + c).sqrt() + b);
    let mut transmission = DVec3::ONE;
    (0..SEGMENTS)
        .map(|i| {
            let start = distance * (i as f64 / SEGMENTS as f64).powi(2);
            let end = distance * ((i + 1) as f64 / SEGMENTS as f64).powi(2);
            let point = DVec3::Z * r + direction * ((start + end) * 0.5);
            let radius = point.length();
            let h = (radius - p.radius_m).max(0.0);
            let rayleigh =
                DVec3::from_array(p.rayleigh_rgb_m_inv) * (-h / p.rayleigh_scale_height_m).exp();
            let aerosol = p.mie_m_inv * (-h / p.aerosol_scale_height_m).exp();
            let ozone = (1.0 - (h - p.ozone_peak_height_m).abs() / p.ozone_half_width_m).max(0.0);
            let extinction =
                rayleigh + DVec3::splat(aerosol) + DVec3::from_array(p.ozone_rgb_m_inv) * ozone;
            let tau = extinction * (end - start);
            let integral =
                transmission * tau.map(|t| -(-t).exp_m1()) / extinction.max(DVec3::splat(1e-30));
            transmission *= (-tau).exp();
            Segment {
                up: point / radius,
                horizon: -(h * (2.0 * p.radius_m + h)).sqrt() / radius,
                optical_row: (h / p.atmospheric_top_m).clamp(0.0, 1.0).sqrt()
                    * f64::from(super::atmosphere_table::HEIGHT - 1),
                rayleigh: integral * rayleigh,
                aerosol: integral * (0.9 * aerosol),
            }
        })
        .collect()
}

fn incoming(sample: &Segment, source: DVec3, optical: &[DVec3]) -> DVec3 {
    let mu = source.dot(sample.up);
    if mu <= sample.horizon {
        return DVec3::ZERO;
    }
    let width = super::atmosphere_table::WIDTH as usize;
    let height = super::atmosphere_table::HEIGHT as usize;
    let x = (((mu - sample.horizon) / (1.0 - sample.horizon)).clamp(0.0, 1.0)).sqrt()
        * (width - 1) as f64;
    let y = sample.optical_row;
    let row = |i: usize| {
        let offset = i * width;
        optical[offset + x as usize]
            .lerp(optical[offset + (x as usize + 1).min(width - 1)], x.fract())
    };
    row(y as usize).lerp(row((y as usize + 1).min(height - 1)), y.fract())
}

pub(super) fn pixels(p: Optics, depth: &[u16]) -> Vec<u16> {
    let optical: Vec<_> = depth
        .chunks_exact(4)
        .map(|v| {
            DVec3::from_array(std::array::from_fn(|i| {
                (-half::f16::from_bits(v[i]).to_f64()).exp()
            }))
        })
        .collect();
    let sources: Vec<_> = (0..WIDTH)
        .map(|x| {
            let u = 2.0 * f64::from(x) / f64::from(WIDTH - 1) - 1.0;
            let mu = u * u.abs();
            DVec3::new((1.0 - mu * mu).sqrt(), 0.0, mu)
        })
        .collect();
    let mut result = Vec::with_capacity((WIDTH * HEIGHT * 4) as usize);
    for y in 0..HEIGHT {
        let h = (f64::from(y) / f64::from(HEIGHT - 1)).powi(2) * p.atmospheric_top_m;
        let mut row = [DVec3::ZERO; WIDTH as usize];
        for i in 0..ZENITH {
            // Uniform in cos²(theta) integrates cos(theta) dOmega with weight PI/N.
            let z = ((i as f64 + 0.5) / ZENITH as f64).sqrt();
            let horizontal = (1.0 - z * z).sqrt();
            let path = view_path(p, h, DVec3::new(horizontal, 0.0, z));
            for a in 0..AZIMUTH {
                let phi = TAU * (a as f64 + 0.5) / AZIMUTH as f64;
                for (sum, &source) in row.iter_mut().zip(&sources) {
                    let rotated = DVec3::new(source.x * phi.cos(), source.x * phi.sin(), source.z);
                    let cosine = horizontal * rotated.x + z * rotated.z;
                    let rayleigh = 3.0 * (1.0 + cosine * cosine) / (16.0 * PI);
                    let g = p.mie_anisotropy;
                    let mie =
                        (1.0 - g * g) / (4.0 * PI * (1.0 + g * g - 2.0 * g * cosine).powf(1.5));
                    for s in &path {
                        *sum += incoming(s, rotated, &optical)
                            * (s.rayleigh * rayleigh + s.aerosol * mie);
                    }
                }
            }
        }
        for sum in row {
            let rgb = sum * (PI / (ZENITH * AZIMUTH) as f64);
            result.extend(rgb.to_array().map(|v| half::f16::from_f64(v).to_bits()));
            result.push(0);
        }
    }
    result
}
