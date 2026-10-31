use super::{
    column,
    grid::{self, Column, Grid, WeatherResolution, ATLAS_LAYERS, CHANNELS, SNAPSHOT_LAYERS},
    region::Region,
    simulated::{HEIGHT, STEP_MS, WIDTH},
    terrain::{self, TerrainConfig, TerrainInfluence},
    WeatherSample,
};

#[derive(Clone, Copy, PartialEq)]
pub(super) struct Request {
    pub(super) seed: u128,
    pub(super) clock_revision: Option<u64>,
    pub(super) epoch: i64,
    pub(super) region: Region,
    pub(super) terrain: Option<TerrainConfig>,
    pub(super) resolution: WeatherResolution,
    pub(super) correction: Option<(f64, f64, WeatherSample)>,
}

pub(super) struct Maps {
    pub(super) request: Request,
    pub(super) pixels: [Vec<u8>; ATLAS_LAYERS],
}

#[derive(Default)]
pub(super) struct TerrainCache {
    config: Option<TerrainConfig>,
    resolution: WeatherResolution,
    terrain: Vec<TerrainInfluence>,
    global_surface: Vec<TerrainInfluence>,
    local_surface: Option<(Region, Vec<TerrainInfluence>)>,
    columns: Option<(Request, [Grid; 2])>,
    available: bool,
}

impl TerrainCache {
    pub(super) fn build(&mut self, request: Request) -> Maps {
        let available = minimium_procgen::try_earth_elevation_m([1.0, 0.0, 0.0]).is_ok();
        if self.global_surface.is_empty()
            || self.config != request.terrain
            || self.available != available
        {
            self.global_surface = terrain::map(request.terrain, Region::GLOBAL);
            self.local_surface = None;
        }
        if self.terrain.is_empty()
            || self.config != request.terrain
            || self.available != available
            || self.resolution != request.resolution
        {
            self.terrain = grid::terrain_grid(request);
            self.columns = None;
            self.config = request.terrain;
            self.resolution = request.resolution;
            self.available = available;
        }
        if self
            .local_surface
            .as_ref()
            .is_none_or(|(region, _)| *region != request.region)
        {
            self.local_surface = Some((
                request.region,
                terrain::map(request.terrain, request.region),
            ));
        }
        let mut key = request;
        key.region = Region::GLOBAL;
        key.correction = None;
        if self.columns.as_ref().is_none_or(|(old, _)| *old != key) {
            self.columns = Some((
                key,
                std::array::from_fn(|snapshot| {
                    let days =
                        (request.epoch + snapshot as i64) as f64 * STEP_MS as f64 / 86_400_000.0;
                    Grid::build(request, days, &self.terrain, request.resolution.width())
                }),
            ));
        }
        bake(
            request,
            &self.columns.as_ref().unwrap().1,
            [
                &self.global_surface,
                &self.local_surface.as_ref().unwrap().1,
            ],
        )
    }
}

pub(super) fn bake(
    request: Request,
    columns: &[Grid; 2],
    surfaces: [&[TerrainInfluence]; 2],
) -> Maps {
    let mut pixels = std::array::from_fn(|_| vec![0; WIDTH * HEIGHT * 4]);
    for (patch, region) in [Region::GLOBAL, request.region].into_iter().enumerate() {
        if patch == 1 && region == Region::GLOBAL {
            let (global, local) = pixels.split_at_mut(SNAPSHOT_LAYERS);
            for (local, global) in local.iter_mut().zip(global) {
                local.copy_from_slice(global);
            }
            break;
        }
        for (map, grid) in columns.iter().enumerate() {
            let state = super::wind::WindState::new(
                (request.epoch + map as i64) as f64 * STEP_MS as f64 / 1000.0,
                request.seed,
            );
            for y in 0..HEIGHT {
                for x in 0..WIDTH {
                    let (lat, lon) = region.coordinates(x, y);
                    let mut values = grid.sample(lat, lon);
                    if let Some(ground) = surfaces[patch].get(y * WIDTH + x) {
                        let up = glam::DVec3::new(
                            lat.cos() * lon.cos(),
                            lat.sin(),
                            lat.cos() * lon.sin(),
                        );
                        column::refine(&mut values, *ground, state, up);
                    }
                    if let Some((latitude, longitude, sample)) = request.correction {
                        let angular = (lat.sin() * latitude.to_radians().sin()
                            + lat.cos()
                                * latitude.to_radians().cos()
                                * (lon - longitude.to_radians()).cos())
                        .clamp(-1.0, 1.0)
                        .acos();
                        let weight = (-(angular / 0.035).powi(2)).exp();
                        let target = [
                            sample.low,
                            sample.middle,
                            sample.high,
                            sample.rain_mm_h,
                            sample.humidity,
                            sample.temperature_c,
                            sample.fog,
                            sample.ground_moisture,
                            sample.wind_east_m_s,
                            sample.wind_north_m_s,
                        ];
                        for (channel, target) in target.into_iter().enumerate() {
                            values[channel] += (f64::from(target) - values[channel]) * weight;
                        }
                        for layer in 0..3 {
                            let condensate = 0.2 + (f64::from(sample.rain_mm_h) / 20.0).min(0.6);
                            values[14 + layer * 4] +=
                                (condensate - values[14 + layer * 4]) * weight;
                        }
                    }
                    for (channel, pixel) in encode(values).into_iter().enumerate() {
                        let index = (y * WIDTH + x) * 4;
                        pixels[patch * SNAPSHOT_LAYERS + channel * 2 + map][index..index + 4]
                            .copy_from_slice(&pixel);
                    }
                }
            }
        }
    }
    Maps { request, pixels }
}

pub(super) fn encode(mut values: Column) -> [[u8; 4]; CHANNELS / 4] {
    values[3] /= 32.0;
    values[5] = (values[5] + 40.0) / 80.0;
    values[8] = (values[8] + 32.0) / 64.0;
    values[9] = (values[9] + 32.0) / 64.0;
    values[11] /= 20000.0;
    for layer in 0..3 {
        values[12 + layer * 4] /= 20000.0;
        values[13 + layer * 4] /= 20000.0;
    }
    for value in &mut values[24..28] {
        *value = (*value + 48.0) / 96.0;
    }
    values[28] = values[28] * 0.5 + 0.5;
    values[29] = values[29] * 0.5 + 0.5;
    values[30] += 0.5;
    values[31] += 0.5;
    std::array::from_fn(|map| {
        std::array::from_fn(|channel| {
            (values[map * 4 + channel].clamp(0.0, 1.0) * 255.0).round() as u8
        })
    })
}

pub(super) fn sample(
    pixels: &[Vec<u8>; ATLAS_LAYERS],
    uv: [f64; 2],
    patch: usize,
    channel: usize,
    blend: f32,
) -> f32 {
    let x = uv[0] * WIDTH as f64 - 0.5;
    let y = uv[1] * HEIGHT as f64 - 0.5;
    let x0 = x.floor() as isize;
    let y0 = y.floor() as isize;
    let tx = (x - x.floor()) as f32;
    let ty = (y - y.floor()) as f32;
    let read = |map: usize, xi: isize, yi: isize| {
        let px = if patch == 0 {
            xi.rem_euclid(WIDTH as isize)
        } else {
            xi.clamp(0, WIDTH as isize - 1)
        } as usize;
        let py = yi.clamp(0, HEIGHT as isize - 1) as usize;
        pixels[patch * SNAPSHOT_LAYERS + channel / 4 * 2 + map][(py * WIDTH + px) * 4 + channel % 4]
            as f32
            / 255.0
    };
    let bilinear = |map| {
        let top = read(map, x0, y0) * (1.0 - tx) + read(map, x0 + 1, y0) * tx;
        let bottom = read(map, x0, y0 + 1) * (1.0 - tx) + read(map, x0 + 1, y0 + 1) * tx;
        top * (1.0 - ty) + bottom * ty
    };
    bilinear(0) * (1.0 - blend) + bilinear(1) * blend
}
