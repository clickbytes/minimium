#[derive(Clone, Copy)]
pub(crate) struct LightningEvent {
    pub(crate) id: u64,
    pub(crate) direction: [f32; 3],
    pub(crate) intensity: f32,
}

pub(crate) fn event(
    seed: u128,
    utc_ms: i64,
    location: (f64, f64),
    mut conditions_at: impl FnMut(f64, f64) -> (f32, f32, Option<u64>),
) -> Option<LightningEvent> {
    let slot = utc_ms.div_euclid(8_000);
    let age = utc_ms.rem_euclid(8_000);
    if age > 180 {
        return None;
    }
    let origin_lat = (location.0 * 10.0).floor() as i64;
    let origin_lon = (location.1 * 10.0).floor() as i64;
    let mut selected = None;
    for north in -2..=2 {
        for east in -2..=2 {
            let cell_lat = (origin_lat + north).clamp(-900, 899);
            let cell_lon = (origin_lon + east + 1800).rem_euclid(3600) - 1800;
            let latitude = (cell_lat as f64 + 0.5) / 10.0;
            let longitude = (cell_lon as f64 + 0.5) / 10.0;
            let (rain, storm, cloud_id) = conditions_at(latitude, longitude);
            let Some(cloud_id) = cloud_id else {
                continue;
            };
            if rain < 1.0 || storm < 0.1 {
                continue;
            }
            let id = mix64(
                (seed as u64)
                    ^ cloud_id
                    ^ ((seed >> 64) as u64)
                    ^ (slot as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15)
                    ^ (cell_lat as u64).rotate_left(17)
                    ^ (cell_lon as u64).rotate_left(41),
            );
            if id % 100 > (rain * 8.0 * storm).min(45.0) as u64 {
                continue;
            }
            let delta_lon = ((longitude - location.1 + 180.0).rem_euclid(360.0) - 180.0)
                * location.0.to_radians().cos();
            let distance = (latitude - location.0).powi(2) + delta_lon.powi(2);
            if selected.is_some_and(|(best, _)| distance >= best) {
                continue;
            }
            let lat = latitude.to_radians();
            let lon = longitude.to_radians();
            selected = Some((
                distance,
                LightningEvent {
                    id,
                    direction: [
                        (lat.cos() * lon.cos()) as f32,
                        lat.sin() as f32,
                        (lat.cos() * lon.sin()) as f32,
                    ],
                    intensity: (1.0 - age as f32 / 180.0).powi(2),
                },
            ));
        }
    }
    selected.map(|(_, event)| event)
}

fn mix64(mut value: u64) -> u64 {
    value ^= value >> 30;
    value = value.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value ^= value >> 27;
    value = value.wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}
