use minimium_astronomy::SkyState;

/// Transforms a world-space observer position into the catalogue epoch frame.
pub fn observer_epoch_position(sky: &SkyState, observer_world_m: [f64; 3]) -> [f64; 3] {
    std::array::from_fn(|column| {
        (0..3)
            .map(|row| sky.catalog_epoch_to_world[row][column] * observer_world_m[row])
            .sum()
    })
}
