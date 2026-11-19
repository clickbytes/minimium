use super::{
    cache::{MeshStats, Morph},
    worker::create_weather_model,
    Publication, Request,
};
use glam::DVec3;
use minimium_environment::weather::{
    CloudSnapshot, SeasonalProfile, CLOUD_OBSERVER_MARGIN, MAX_CLOUDS,
};
use minimium_procgen::GenerationError;
use minimium_renderer::surface_cover::CoverGrid;
use std::sync::Arc;

impl Publication {
    pub(super) fn interval(&self) -> (f64, f64) {
        self.snapshot.clouds.iter().zip(&self.morphs).fold(
            (self.snapshot.valid_from, self.snapshot.valid_until),
            |(lo, hi), (cloud, morph)| {
                let from = cloud.birth_utc + morph.start;
                (lo.max(from), hi.min(from + morph.step))
            },
        )
    }
    pub(super) fn near(&self, observer: DVec3) -> bool {
        self.snapshot.observer.dot(observer) >= CLOUD_OBSERVER_MARGIN.cos()
    }
    pub(super) fn valid_at(&self, utc: f64, observer: DVec3) -> bool {
        let (lo, hi) = self.interval();
        utc >= lo && utc < hi && self.near(observer)
    }
}

pub(super) fn fallback(
    request: Request,
    previous: Option<&Publication>,
    need_cover: bool,
) -> Result<(Arc<Publication>, Option<Arc<CoverGrid>>), GenerationError> {
    let mut model = create_weather_model(
        request.stamp.manifest,
        SeasonalProfile::Seasonal,
        request.terrain_inputs.clone(),
    )?;
    model
        .set_scenario((*request.climate).clone())
        .map_err(|_| GenerationError::invalid_manifest("invalid climate scenario"))?;
    // Local and worker geometry for the same request must have distinct upload keys.
    let mut snapshot = model.query_local(request.observer, request.utc, request.revision + 1)?;
    let previous = previous.filter(|p| p.request.stamp == request.stamp);
    if let Some(previous) = previous {
        retain_regional_clouds(&mut snapshot, previous, model.radius_m());
    }
    let cover = if need_cover {
        Some(Arc::new(CoverGrid::local_column(
            &model,
            request.observer,
            request.utc,
        )?))
    } else {
        None
    };
    let mut faces = Vec::with_capacity(snapshot.clouds.len() * 6);
    let mut morphs = Vec::with_capacity(snapshot.clouds.len());
    let mut stats = MeshStats::default();
    for (index, cloud) in snapshot.clouds.iter().copied().enumerate() {
        let (_, start, step) = cloud.morph_window(snapshot.utc);
        let retained = previous.and_then(|p| {
            let old = p
                .snapshot
                .clouds
                .binary_search_by_key(&cloud.id, |c| c.id)
                .ok()?;
            let morph = p.morphs[old];
            if morph.start != start || morph.step != step || p.request.detail != request.detail {
                return None;
            }
            // Publications group unsorted faces by cloud index. Keep enough room
            // for an envelope for every remaining cloud when the roster grows.
            let first = p.faces.partition_point(|f| (f.center[3] as usize) < old);
            let last = p.faces.partition_point(|f| (f.center[3] as usize) <= old);
            let reserve = (snapshot.clouds.len() - index - 1) * 6;
            (faces.len() + last - first + reserve <= super::MAX_FACES)
                .then_some((morph, &p.faces[first..last]))
        });
        let morph = if let Some((morph, old_faces)) = retained {
            stats.cached += 1;
            faces.extend(old_faces.iter().copied().map(|mut face| {
                face.center[3] = index as f32;
                face
            }));
            morph
        } else {
            stats.simplified += 1;
            for mut face in minimium_renderer::cloud_mesh::envelope(cloud, [0.25; 2]) {
                face.center[3] = index as f32;
                faces.push(face);
            }
            Morph {
                start,
                step,
                level: 0,
            }
        };
        stats.levels[usize::from(morph.level)] += 1;
        morphs.push(morph);
    }
    Ok((
        Arc::new(Publication {
            request,
            snapshot,
            faces,
            morphs,
            coarse: true,
            build_ms: 0.0,
            limited: true,
            stats,
        }),
        cover,
    ))
}

/// A late mesh job cannot remove surviving clouds that drifted in from neighbouring regions.
fn retain_regional_clouds(snapshot: &mut CloudSnapshot, previous: &Publication, radius: f64) {
    let mut clouds = std::collections::BTreeMap::new();
    for cloud in previous.snapshot.clouds.iter().copied() {
        if cloud.in_view_during(
            snapshot.observer,
            radius,
            snapshot.valid_from,
            snapshot.valid_until,
        ) {
            clouds.insert(cloud.id, cloud);
        }
    }
    for cloud in snapshot.clouds.drain(..) {
        clouds.insert(cloud.id, cloud);
    }
    snapshot.clouds = clouds.into_values().take(MAX_CLOUDS).collect();
}

pub(super) fn sort(
    faces: &mut [super::Face],
    publication: &Publication,
    radius: f64,
    utc: f64,
    eye: DVec3,
) {
    let transforms: Vec<_> = publication
        .snapshot
        .clouds
        .iter()
        .map(|c| {
            let up = c.direction_at(utc);
            let east = glam::DVec3::new(-up.z, 0.0, up.x).normalize_or_zero();
            (up * (radius + c.base_m), up, east, up.cross(east))
        })
        .collect();
    faces.sort_by_cached_key(|face| {
        let (center, up, east, north) = transforms[face.center[3] as usize];
        let p = center
            + east * f64::from(face.center[0])
            + up * f64::from(face.center[1])
            + north * f64::from(face.center[2]);
        std::cmp::Reverse((p - eye).length_squared().to_bits())
    });
}
