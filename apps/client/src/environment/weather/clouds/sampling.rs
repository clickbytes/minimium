use super::{density, types::CloudColumnSample, CloudFieldSnapshot, CloudId, CloudLobe};
use crate::environment::weather::wind::{smooth, EARTH_RADIUS};
use glam::DVec3;
use std::collections::BTreeMap;

pub(crate) fn interpolate(a: &CloudLobe, b: &CloudLobe, t: f64) -> CloudLobe {
    let mut lobe = *a;
    lobe.center = a.center.lerp(b.center, t);
    lobe.axes = (a.axes.ln().lerp(b.axes.ln(), t)).exp();
    lobe.orientation = a.orientation.slerp(b.orientation, t).normalize();
    let normal = a.base_normal.lerp(b.base_normal, t);
    lobe.base_normal = normal.normalize();
    lobe.base_offset = (a.base_offset + (b.base_offset - a.base_offset) * t) / normal.length();
    lobe.mass = a.mass + (b.mass - a.mass) * t;
    lobe.density = lobe.mass / density::volume(&lobe).max(1.0);
    lobe.storm = a.storm + (b.storm - a.storm) * t;
    lobe.rain_potential = a.rain_potential + (b.rain_potential - a.rain_potential) * t;
    lobe
}

pub(crate) fn paired(a: Vec<CloudLobe>, b: Vec<CloudLobe>) -> Vec<[CloudLobe; 2]> {
    let mut pairs: BTreeMap<CloudId, [Option<CloudLobe>; 2]> = BTreeMap::new();
    for lobe in a {
        pairs.entry(lobe.id).or_default()[0] = Some(lobe);
    }
    for lobe in b {
        pairs.entry(lobe.id).or_default()[1] = Some(lobe);
    }
    pairs
        .into_values()
        .map(|pair| match pair {
            [Some(a), Some(b)] => [a, b],
            [Some(a), None] => [
                a,
                CloudLobe {
                    density: 0.0,
                    mass: 0.0,
                    ..a
                },
            ],
            [None, Some(b)] => [
                CloudLobe {
                    density: 0.0,
                    mass: 0.0,
                    ..b
                },
                b,
            ],
            _ => unreachable!(),
        })
        .collect()
}

pub(crate) fn column(lobes: impl Iterator<Item = CloudLobe>, up: DVec3) -> CloudColumnSample {
    let origin = up * EARTH_RADIUS;
    let mut result = CloudColumnSample {
        base_m: [20000.0; 3],
        ..Default::default()
    };
    let mut rain_tau = 0.0;
    let mut rain_weight = 0.0;
    let mut strongest_storm = 0.0;
    for lobe in lobes {
        let optical = density::integral(&lobe, origin, up, 0.0, 22000.0);
        if optical < 1e-8 {
            continue;
        }
        let layer = usize::from(lobe.layer);
        let Some((entry, exit)) = density::interval(&lobe, origin, up) else {
            continue;
        };
        result.optical_depth[layer] += optical;
        result.base_m[layer] = result.base_m[layer].min(entry);
        result.top_m[layer] = result.top_m[layer].max(exit);
        if layer != 2 {
            rain_tau += optical;
            rain_weight += lobe.rain_potential * optical;
            result.storm += lobe.storm * optical;
            if lobe.storm * optical > strongest_storm {
                strongest_storm = lobe.storm * optical;
                let mut id = lobe.id;
                id.path = 1;
                id.part_population = 0;
                result.storm_id = Some(id);
            }
        }
    }
    result.coverage = result.optical_depth.map(|tau| smooth(0.005, 0.15, tau));
    let host = smooth(0.15, 1.0, rain_tau);
    result.rain_mm_h = rain_weight / rain_tau.max(1e-8) * host;
    result.storm = (result.storm / rain_tau.max(1e-8) * host).clamp(0.0, 1.0);
    result
}

impl CloudFieldSnapshot {
    pub(crate) fn column(&self, up: DVec3) -> CloudColumnSample {
        column(self.lobes(up, 0.0).into_iter(), up)
    }

    pub(crate) fn point_density(&self, point: DVec3) -> f64 {
        self.lobes(point.normalize(), 0.0)
            .iter()
            .map(|l| density::point(l, point))
            .sum()
    }

    pub(crate) fn ray_optical(&self, origin: DVec3, ray: DVec3, length: f64) -> f64 {
        let midpoint = origin + ray * length * 0.5;
        self.lobes(midpoint.normalize(), length * 0.5)
            .iter()
            .map(|l| density::integral(l, origin, ray, 0.0, length))
            .sum()
    }
}
