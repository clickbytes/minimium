//! Area coverage and occupied optical depth are deliberately separate moments.
use super::{types::CloudColumnSample, CloudFieldSnapshot};
use crate::environment::weather::wind;
use glam::DVec3;

pub(crate) fn area(field: &CloudFieldSnapshot, up: DVec3, radius_m: f64) -> CloudColumnSample {
    if radius_m < 1000.0 {
        return field.column(up);
    }
    let (east, north) = wind::basis(up);
    let mut result = CloudColumnSample::default();
    let mut transmission = [0.0; 3];
    for i in 0..8 {
        let angle = i as f64 * 2.399963229728653;
        let distance = radius_m * ((i as f64 + 0.5) / 8.0).sqrt();
        let point = wind::displace(up, (east * angle.cos() + north * angle.sin()) * distance);
        let sample = field.column(point);
        for (layer, trans) in transmission.iter_mut().enumerate() {
            result.coverage[layer] += sample.coverage[layer] / 8.0;
            *trans += sample.coverage[layer] * (-sample.optical_depth[layer]).exp() / 8.0;
            result.base_m[layer] += sample.base_m[layer] * sample.coverage[layer] / 8.0;
            result.top_m[layer] += sample.top_m[layer] * sample.coverage[layer] / 8.0;
        }
        result.rain_mm_h += sample.rain_mm_h / 8.0;
        result.storm += sample.storm / 8.0;
    }
    for (layer, trans) in transmission.iter().enumerate() {
        let coverage = result.coverage[layer].max(1e-8);
        result.optical_depth[layer] = -(trans / coverage).clamp(1e-8, 1.0).ln();
        result.base_m[layer] /= coverage;
        result.top_m[layer] /= coverage;
        if result.coverage[layer] < 1e-8 {
            let parameters = field.atmosphere.column(up, field.blend)[3 + layer];
            result.base_m[layer] = f64::from(parameters.x) * 20000.0;
            result.top_m[layer] = f64::from(parameters.y) * 20000.0;
        }
    }
    result
}
