use super::types::random;
use crate::environment::weather::wind::smooth;

pub(super) fn weight(age: f64, phase: f64) -> f64 {
    smooth(phase, 0.85 + phase, age) * (1.0 - smooth(1.85 + phase, 3.0, age))
}

pub(super) fn parts(seconds: f64, key: u64, layer: u8, convection: f64) -> Vec<(i64, f64, f64)> {
    let populations = if layer == 2 {
        [(7200.0, 1.0), (1200.0, 0.0)]
    } else {
        [(5400.0, 1.0 - convection), (1200.0, convection)]
    };
    let mut result = Vec::with_capacity(6);
    for (regime, (cadence, share)) in populations.into_iter().enumerate() {
        if share <= 0.0 {
            continue;
        }
        let phase = random(key, 923 + regime as u64) * 0.35;
        let epoch = (seconds / cadence).floor() as i64;
        let mut values: [(i64, f64, f64); 3] = std::array::from_fn(|i| {
            let id = epoch - i as i64;
            let age = seconds / cadence - id as f64;
            (id * 2 + regime as i64, weight(age, phase), age)
        });
        let total: f64 = values.iter().map(|v| v.1).sum();
        for value in &mut values {
            value.1 *= share / total.max(1e-12);
        }
        result.extend(values);
    }
    result
}
