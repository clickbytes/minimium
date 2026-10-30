/// Broad terrain palette; elevation is measured above the configured sea level.
pub(crate) fn terrain_color(elevation_mm: i64, ocean: bool) -> [f32; 3] {
    if ocean {
        return [0.055, 0.20, 0.43];
    }
    let metres = elevation_mm as f32 / 1000.0;
    let stops = [
        (0.0, [0.53, 0.49, 0.31]),
        (20.0, [0.20, 0.43, 0.19]),
        (70.0, [0.28, 0.39, 0.21]),
        (130.0, [0.45, 0.41, 0.34]),
        (220.0, [0.65, 0.64, 0.61]),
        (320.0, [0.92, 0.92, 0.90]),
    ];
    if metres <= stops[0].0 {
        return stops[0].1;
    }
    for pair in stops.windows(2) {
        if metres <= pair[1].0 {
            let blend = (metres - pair[0].0) / (pair[1].0 - pair[0].0);
            return std::array::from_fn(|axis| {
                pair[0].1[axis] * (1.0 - blend) + pair[1].1[axis] * blend
            });
        }
    }
    stops[stops.len() - 1].1
}

pub(crate) fn mostly_ocean(samples: [bool; 3]) -> bool {
    samples.into_iter().filter(|sample| *sample).count() >= 2
}
