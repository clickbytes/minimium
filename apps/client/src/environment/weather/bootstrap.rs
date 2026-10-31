//! Small global field while the regional worker reconstructs terrain detail.
use super::{
    grid::Grid,
    maps::{self, Maps, Request},
    region::Region,
    simulated::STEP_MS,
};

pub(super) fn build(mut request: Request) -> Maps {
    request.region = Region::GLOBAL;
    let surface = minimium_weather::WeatherSurface::earth();
    let columns = std::array::from_fn(|snapshot| {
        let days = (request.epoch + snapshot as i64) as f64 * STEP_MS as f64 / 86_400_000.0;
        Grid::build(request, days, &[], 32, &surface)
    });
    maps::bake(request, &columns, [&[], &[]])
}
