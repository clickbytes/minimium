//! Weather addresses deliberately use 128 cells per face, not terrain sectors.
use glam::DVec3;
use minimium_core::world::{
    CubeSampleKey, PlanetFace, PlanetSectorKey, PLANET_SAMPLE_COORDINATE_SCALE,
};
use std::collections::{BTreeSet, VecDeque};

pub(crate) const COUNT: u16 = 128;
// Maximum centre-to-corner angle of any weather region.
const BOUND: f64 = 0.012;
pub(crate) type Region = PlanetSectorKey;

pub(crate) fn at(direction: DVec3) -> Region {
    Region::from_direction_with_count(direction.as_vec3().to_array(), COUNT)
}

pub(crate) fn point(region: Region, u: f64, v: f64) -> DVec3 {
    let (x, y) = region.coordinates();
    let u = (f64::from(x) + u) * 2.0 / f64::from(COUNT) - 1.0;
    let v = (f64::from(y) + v) * 2.0 / f64::from(COUNT) - 1.0;
    match region.face() {
        PlanetFace::PositiveX => DVec3::new(1.0, u, v),
        PlanetFace::NegativeX => DVec3::new(-1.0, u, v),
        PlanetFace::PositiveY => DVec3::new(u, 1.0, v),
        PlanetFace::NegativeY => DVec3::new(u, -1.0, v),
        PlanetFace::PositiveZ => DVec3::new(u, v, 1.0),
        PlanetFace::NegativeZ => DVec3::new(u, v, -1.0),
    }
    .normalize()
}

pub(crate) fn surface_key(direction: DVec3) -> CubeSampleKey {
    let region = at(direction);
    let p = direction / direction.abs().max_element();
    let (u, v) = match region.face() {
        PlanetFace::PositiveX | PlanetFace::NegativeX => (p.y, p.z),
        PlanetFace::PositiveY | PlanetFace::NegativeY => (p.x, p.z),
        PlanetFace::PositiveZ | PlanetFace::NegativeZ => (p.x, p.y),
    };
    let coordinate =
        |x: f64| (x.clamp(-1.0, 1.0) * f64::from(PLANET_SAMPLE_COORDINATE_SCALE)).round() as i32;
    region
        .face()
        .cube_sample(coordinate(u), coordinate(v))
        .expect("normalized cube sample")
}

/// Conservative cap intersection includes neighbouring faces and pole regions.
pub(crate) fn intersecting(center: DVec3, angle: f64) -> Vec<Region> {
    let first = at(center);
    let mut seen = BTreeSet::from([first]);
    let mut pending = VecDeque::from([first]);
    let mut result = Vec::new();
    while let Some(region) = pending.pop_front() {
        if point(region, 0.5, 0.5).dot(center) < (angle + BOUND).cos() {
            continue;
        }
        result.push(region);
        for next in region.neighbors_with_count(COUNT) {
            if seen.insert(next) {
                pending.push_back(next);
            }
        }
    }
    result.sort();
    result
}

pub(crate) fn address(region: Region) -> u64 {
    let (u, v) = region.coordinates();
    (u64::from(region.face().index()) * u64::from(COUNT) + u64::from(u)) * u64::from(COUNT)
        + u64::from(v)
}
