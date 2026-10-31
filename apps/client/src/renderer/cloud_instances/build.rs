use super::{diagnostics::Diagnostics, gpu, lod, CloudInstanceGpu, LodHistory, Request};
use crate::environment::weather::clouds::{runtime::PresentedCloudFrame, sampling, shapes};
use std::time::Instant;

pub(super) struct Built {
    pub(super) depths: LodHistory,
    pub(super) request: Request,
    pub(super) frame: PresentedCloudFrame,
    pub(super) instances: Vec<CloudInstanceGpu>,
    pub(super) far: super::super::cloud_mesh::Built,
    pub(super) shadows: Vec<u8>,
    pub(super) region: [f32; 4],
    pub(super) diagnostics: Diagnostics,
}

pub(super) fn build(request: Request, retained: &mut super::cache::BuildCache) -> Built {
    let started = Instant::now();
    let physical_start = retained.physical_seconds;
    let history = &request.previous_lod;
    let fields = std::array::from_fn(|i| {
        let seconds = request.start_seconds + i as f64 * request.duration;
        retained.field(&request, seconds)
    });
    let selection = lod::select(&request, &fields, history);
    // Both cuts are rebuilt from these SAME physical endpoints. LOD never blends
    // a previous weather revision into the newly published rain or shadow field.
    let previous_cost: usize = selection
        .clusters
        .iter()
        .map(|pair| {
            let c = pair[0].as_ref().or(pair[1].as_ref()).unwrap();
            lod::cost(history.get(&c.id).copied().unwrap_or(0))
        })
        .sum();
    let old_fits = previous_cost <= request.settings.instances as usize / 2;
    let mut instances = Vec::new();
    for sign in [-1.0, 1.0] {
        let begin = instances.len();
        let mut group_ranges = Vec::new();
        for pair in &selection.clusters {
            let c = pair[0].as_ref().or(pair[1].as_ref()).unwrap();
            let depth = if sign < 0.0 && old_fits {
                history.get(&c.id).copied().unwrap_or(0)
            } else {
                selection.depths[&c.id]
            };
            let a = pair[0]
                .as_ref()
                .map_or_else(Vec::new, |c| shapes::lobes(c, depth));
            let b = pair[1]
                .as_ref()
                .map_or_else(Vec::new, |c| shapes::lobes(c, depth));
            let start = instances.len();
            instances.extend(
                sampling::paired(a, b)
                    .iter()
                    .map(|p| gpu::instance(p, request.eye, request.radius, sign)),
            );
            group_ranges.push(start..instances.len());
        }
        for range in group_ranges {
            for i in range.clone() {
                let center = glam::Vec3::from_array(instances[i].center_a[..3].try_into().unwrap());
                let mut neighbors: Vec<_> = range.clone().filter(|&j| j != i).collect();
                neighbors.sort_by(|&a, &b| {
                    let at = |j: usize| {
                        center.distance_squared(glam::Vec3::from_array(
                            instances[j].center_a[..3].try_into().unwrap(),
                        ))
                    };
                    at(a).total_cmp(&at(b))
                });
                let indices: [u32; 8] =
                    std::array::from_fn(|n| neighbors.get(n).map_or(u32::MAX, |&j| j as u32));
                instances[i].neighbors_a.copy_from_slice(&indices[..4]);
                instances[i].neighbors_b.copy_from_slice(&indices[4..]);
            }
        }
        debug_assert!(instances.len() - begin <= request.settings.instances as usize / 2);
    }
    let cache = retained.moments(&fields, request.atlas.region);
    let far = super::super::cloud_mesh::build(
        request.eye,
        request.radius,
        request.pixels_per_radian,
        request.settings.aggregates,
        &cache,
    );
    let diagnostics = Diagnostics {
        physical_seconds: retained.physical_seconds - physical_start,
        families: std::array::from_fn(|family| {
            selection
                .clusters
                .iter()
                .filter(|p| p[0].as_ref().or(p[1].as_ref()).unwrap().family as usize == family)
                .count()
        }),
        eye_density: fields[0].point_density(request.eye * (6371000.0 / request.radius)),
        vertical_optical: fields[0].ray_optical(
            request.eye.normalize() * 6371000.0,
            request.eye.normalize(),
            22000.0,
        ),
        clusters: selection.clusters.len(),
        instances: instances.len(),
        aggregates: far.tiles,
        merged: selection.merged,
        milliseconds: started.elapsed().as_secs_f64() * 1000.0,
        cache_bytes: fields.iter().map(|f| f.bytes()).sum::<usize>() + cache.pixels.len(),
    };
    Built {
        depths: selection.depths,
        request,
        frame: PresentedCloudFrame { fields },
        instances,
        far,
        shadows: cache.pixels.clone(),
        region: selection.region,
        diagnostics,
    }
}
