//! Conservative raster bounds for a spherical cap. Bilinear interpolation cannot
//! exceed its contributing pixels. One-degree maxima are filled lazily, off-thread.
use super::PreparedEarthGeography;
use minimium_procgen::EarthSampleError;
use std::sync::atomic::{AtomicI32, Ordering};

const CELLS: usize = 360 * 180;

pub(super) fn earth_elevation_upper_bound_m(
    package: &PreparedEarthGeography,
    direction: [f64; 3],
    angle: f64,
) -> Result<f64, EarthSampleError> {
    let length = direction.iter().map(|v| v * v).sum::<f64>().sqrt();
    if !length.is_finite() || length <= 0.0 || !angle.is_finite() || !(0.0..=0.2).contains(&angle) {
        return Err(EarthSampleError::InvalidDirection);
    }
    let latitude = (direction[1] / length).clamp(-1.0, 1.0).asin();
    let longitude = direction[2].atan2(direction[0]).to_degrees();
    let lat_span = angle.to_degrees();
    let lon_span = if latitude.abs() + angle >= std::f64::consts::FRAC_PI_2 {
        180.0
    } else {
        (angle.sin() / latitude.cos())
            .clamp(-1.0, 1.0)
            .asin()
            .to_degrees()
    };
    let south = ((latitude.to_degrees() - lat_span).floor() as i32).clamp(-90, 89);
    let north = ((latitude.to_degrees() + lat_span).floor() as i32).clamp(-90, 89);
    let west = (longitude - lon_span).floor() as i32;
    let east = ((longitude + lon_span).floor() as i32).min(west + 359);
    let maxima = package
        .maxima
        .get_or_init(|| (0..CELLS).map(|_| AtomicI32::new(i32::MIN)).collect());
    let samples = package.width as f64 / 360.0;
    let mut bound = i32::MIN;
    for lat in south..=north {
        for raw_lon in west..=east {
            let lon = (raw_lon + 180).rem_euclid(360) - 180;
            let cell = &maxima[((lat + 90) * 360 + lon + 180) as usize];
            let mut maximum = cell.load(Ordering::Relaxed);
            if maximum == i32::MIN {
                // Include all four bilinear support pixels, also at cell borders.
                let x0 = ((f64::from(lon) + 180.0) * samples - 0.5).floor() as i64;
                let x1 = ((f64::from(lon) + 181.0) * samples - 0.5).ceil() as i64;
                let y0 = ((89.0 - f64::from(lat)) * samples - 0.5).floor() as i64;
                let y1 = ((90.0 - f64::from(lat)) * samples - 0.5).ceil() as i64;
                for y in y0..=y1 {
                    for x in x0..=x1 {
                        maximum = maximum.max(i32::from(package.pixel(x, y)));
                    }
                }
                cell.store(maximum, Ordering::Relaxed);
            }
            bound = bound.max(maximum);
        }
    }
    Ok(f64::from(bound))
}
