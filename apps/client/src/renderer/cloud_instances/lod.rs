use super::{LodHistory, Request};
use crate::environment::weather::{
    clouds::{shapes, CloudCluster, CloudFieldSnapshot, CloudId},
    wind::EARTH_RADIUS,
};
use glam::DVec3;
use std::collections::BTreeMap;

pub(super) struct Selection {
    pub(super) clusters: Vec<[Option<CloudCluster>; 2]>,
    pub(super) depths: LodHistory,
    pub(super) region: [f32; 4],
    pub(super) merged: usize,
}

pub(super) fn select(
    request: &Request,
    fields: &[std::sync::Arc<CloudFieldSnapshot>; 2],
    history: &LodHistory,
) -> Selection {
    let physical_eye = request.eye * (EARTH_RADIUS / request.radius);
    let up = physical_eye.normalize_or_zero();
    let altitude = (physical_eye.length() - EARTH_RADIUS).max(0.0);
    let pixels = 25000.0 * request.pixels_per_radian / altitude.max(1000.0);
    let mut radius: f64 = if pixels < request.settings.lod * 0.8 {
        0.0
    } else {
        150000.0
    };
    let budget = request.settings.instances as usize / 2;
    let mut clusters;
    loop {
        clusters = if radius > 0.0 {
            pairs(fields, up, radius)
        } else {
            Vec::new()
        };
        if clusters.len() <= budget {
            break;
        }
        radius *= 0.65;
        if radius < 1000.0 {
            radius = 0.0;
        }
    }
    let mut depths = LodHistory::new();
    let mut used = clusters.len();
    let mut priorities: Vec<_> = clusters
        .iter()
        .map(|pair| {
            let c = pair[0].as_ref().or(pair[1].as_ref()).unwrap();
            let distance =
                (physical_eye.distance(c.up * (EARTH_RADIUS + c.base_m)) - c.width_m).max(100.0);
            (c.id, c.width_m * 2.0 / distance * request.pixels_per_radian)
        })
        .collect();
    priorities.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    let mut merged = 0;
    for (id, pixels) in priorities {
        let old = history.get(&id).copied().unwrap_or(0);
        let mut depth = 0;
        for next in 1..=shapes::DETAIL_DEPTH {
            let threshold = request.settings.lod * if old >= next { 0.8 } else { 1.0 };
            if pixels / f64::from(1_u32 << next) < threshold {
                break;
            }
            let extra = cost(next) - cost(depth);
            if used + extra > budget {
                merged += 1;
                break;
            }
            used += extra;
            depth = next;
        }
        depths.insert(id, depth);
    }
    Selection {
        clusters,
        depths,
        region: up
            .as_vec3()
            .extend((radius / EARTH_RADIUS) as f32)
            .to_array(),
        merged,
    }
}

pub(super) fn cost(depth: u8) -> usize {
    // Union may contain four short-lived populations across an interval boundary.
    if depth == 0 {
        1
    } else {
        8 * (1 << depth)
    }
}

fn pairs(
    fields: &[std::sync::Arc<CloudFieldSnapshot>; 2],
    up: DVec3,
    radius: f64,
) -> Vec<[Option<CloudCluster>; 2]> {
    let mut pairs: BTreeMap<CloudId, [Option<CloudCluster>; 2]> = BTreeMap::new();
    for (time, field) in fields.iter().enumerate() {
        for c in field.clusters(up, radius) {
            let id = c.id;
            pairs.entry(id).or_default()[time] = Some(c);
        }
    }
    pairs.into_values().collect()
}
