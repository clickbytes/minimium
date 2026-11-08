use minimium_procgen::{planet_surface_for, PlanetTerrain, SectorGeometry};
use minimium_world::world::{
    PlanetFace, PlanetSectorKey, SectorKey, WorldManifest, PLANET_SAMPLE_COORDINATE_SCALE,
};
use std::collections::{HashSet, VecDeque};
use std::sync::Arc;

const MAX_VISIBLE_SECTORS: usize = 512;

pub fn render_patches_for_view(
    manifest: WorldManifest,
    view: crate::terrain_view::TerrainView,
    collision: &[(SectorKey, Arc<SectorGeometry>)],
    geography: Option<Arc<dyn minimium_procgen::PreparedGeography>>,
    cache: &mut crate::render_patch_cache::RenderPatchCache,
    cancelled: &dyn Fn() -> bool,
) -> Vec<(SectorKey, u8, Arc<SectorGeometry>)> {
    let position = view.position;
    if manifest.planet().is_some() {
        return dense_render_patches(manifest, view, collision, geography, cache, cancelled);
    }
    let mut terrain = PlanetTerrain::new(manifest.generation(), planet_surface_for(manifest))
        .expect("planet manifest is compatible with the surface");
    if let Some(definition) = manifest.planet() {
        terrain = terrain.with_definition(definition);
    }
    if let Some(geography) = geography.clone() {
        terrain = terrain.with_geography(geography);
    }
    let radius = planet_surface_for(manifest).radius_mm() as f32 / 1000.0;
    let altitude_radius = position.iter().map(|x| x * x).sum::<f32>().sqrt();
    let horizon = (altitude_radius * altitude_radius - radius * radius)
        .max(0.0)
        .sqrt()
        .max(220.0);
    let count = minimium_procgen::planet_sector_count(manifest);
    let mut queue = VecDeque::from([PlanetSectorKey::from_direction_with_count(position, count)]);
    let mut visited = HashSet::new();
    let mut patches = Vec::new();
    while let Some(key) = queue.pop_front() {
        if cancelled() {
            break;
        }
        if !visited.insert(key) || patches.len() >= MAX_VISIBLE_SECTORS {
            continue;
        }
        let sample = key
            .cube_sample_with_count(8, 8, 16, count)
            .expect("sector midpoint is valid");
        let center = terrain
            .sample(sample)
            .position_mm
            .coordinates()
            .map(|x| x as f32 / 1000.0);
        let distance = center
            .iter()
            .zip(position)
            .map(|(a, b)| (a - b) * (a - b))
            .sum::<f32>()
            .sqrt();
        if distance > horizon + 130.0 {
            continue;
        }
        let subdivisions = if distance < 100.0 {
            16
        } else if distance < 500.0 {
            8
        } else if distance < 1_000.0 {
            4
        } else {
            2
        };
        let sector = SectorKey::Planet(key);
        let patch = cache.get(sector, subdivisions as u8, || {
            let mut patch = minimium_procgen::generate_planet_render_patch_with_geography(
                manifest,
                key,
                subdivisions,
                geography.clone(),
            );
            if let Some((_, g)) = collision.iter().find(|(k, _)| *k == sector) {
                patch.boxes = g.boxes.clone();
            }
            patch
        });
        let patch = cache.with_boxes(
            sector,
            subdivisions as u8,
            patch,
            collision
                .iter()
                .find(|(k, _)| *k == sector)
                .map(|(_, g)| g.boxes.as_slice()),
        );
        patches.push((sector, subdivisions as u8, patch));
        queue.extend(key.neighbors_with_count(count));
    }
    patches
}

fn dense_render_patches(
    manifest: WorldManifest,
    view: crate::terrain_view::TerrainView,
    collision: &[(SectorKey, Arc<SectorGeometry>)],
    geography: Option<Arc<dyn minimium_procgen::PreparedGeography>>,
    cache: &mut crate::render_patch_cache::RenderPatchCache,
    cancelled: &dyn Fn() -> bool,
) -> Vec<(SectorKey, u8, Arc<SectorGeometry>)> {
    let count = minimium_procgen::planet_sector_count(manifest);
    let radius = minimium_procgen::planet_surface_for(manifest).radius_mm() as f64 / 1000.0;
    let camera = view.position.map(f64::from);
    let camera_radius = camera.iter().map(|v| v * v).sum::<f64>().sqrt();
    let visibility_radius = radius
        + manifest
            .planet()
            .map_or(0.0, |d| d.sea_level_mm as f64 / 1000.0);
    let horizon = (camera_radius * camera_radius - visibility_radius * visibility_radius)
        .max(0.0)
        .sqrt()
        .max(220.0);
    let relief = manifest.planet().map_or(100.0, |d| {
        (f64::from(d.mountain_limit_mm) + d.sea_level_mm.unsigned_abs() as f64) / 1000.0
    });
    let mut terrain = PlanetTerrain::new(manifest.generation(), planet_surface_for(manifest))
        .expect("valid planet");
    if let Some(d) = manifest.planet() {
        terrain = terrain.with_definition(d);
    }
    if let Some(g) = geography.clone() {
        terrain = terrain.with_geography(g);
    }
    let cell_width = radius * 2.0
        / f64::from(count)
        / f64::from(minimium_procgen::PLANET_RENDER_BLOCK_SUBDIVISIONS);
    let coasts = &mut cache.coasts;
    let mut candidate = |face: PlanetFace, u: u16, v: u16, span: u16| {
        let scale = PLANET_SAMPLE_COORDINATE_SCALE as i64;
        let coordinate = |p: u16| {
            (-scale + scale * (2 * i64::from(p) + i64::from(span)) / i64::from(count)) as i32
        };
        let sample = face
            .cube_sample(coordinate(u), coordinate(v))
            .expect("face sample");
        let point = terrain.sample(sample);
        let center = point.position_mm.coordinates().map(|x| x as f64 / 1000.0);
        let distance = center
            .iter()
            .zip(camera)
            .map(|(x, c)| (x - c).powi(2))
            .sum::<f64>()
            .sqrt();
        let bound = radius * 3.0 * f64::from(span) / f64::from(count) + relief;
        // A raised ridge beyond the ground horizon can still cast into the view.
        // Retain a coarse halo outside the camera frustum; only on-screen leaves
        // compete for detail. The same leaf budget and upload limits still apply.
        let caster_horizon = (2.0 * visibility_radius * relief.max(0.0)).sqrt();
        if distance - bound > horizon + caster_horizon + 100.0 {
            return None;
        }
        let mut score = cell_width * f64::from(span) / distance.max(cell_width * 0.5);
        if !view.intersects(center, bound) || distance - bound > horizon + 100.0 {
            score = 0.0;
        }
        // Probe the mesh lattice only when its classification can affect a split.
        // Cache source coverage independently of camera/LOD hysteresis. In overview,
        // spend detail on coastline silhouettes, then land, before uniform ocean.
        let max_priority = view.surface_priority(crate::coast_probe::SurfaceClass::Coast);
        if terrain.is_earth() && span > 1 && score > view.cell_error_limit() * 0.75 / max_priority {
            score *= view.surface_priority(coasts.classify(&terrain, (face, u, v, span), count));
        }
        Some((face, u, v, span, score))
    };
    let mut leaves: Vec<_> = PlanetFace::ALL
        .iter()
        .filter_map(|f| candidate(*f, 0, 0, count))
        .collect();
    // Refine the most visible error first. Keep every parent until all of its
    // children fit, so the budget cannot create holes or truncate coverage.
    while leaves.len() + 3 <= view.patch_budget() {
        if cancelled() {
            return Vec::new();
        }
        let next = leaves
            .iter()
            .enumerate()
            .filter(|(_, n)| {
                let hysteresis = if cache.refined.contains(&(n.0, n.1, n.2, n.3)) {
                    0.75
                } else {
                    1.0
                };
                n.3 > 1 && n.4 > view.cell_error_limit() * hysteresis
            })
            .max_by(|(_, a), (_, b)| a.4.total_cmp(&b.4))
            .map(|(i, _)| i);
        let Some(index) = next else {
            break;
        };
        let (face, u, v, span, _) = leaves.swap_remove(index);
        let half = span / 2;
        let mut children = Vec::with_capacity(4);
        for dy in [0, half] {
            for dx in [0, half] {
                if let Some(child) = candidate(face, u + dx, v + dy, half) {
                    children.push(child);
                }
            }
        }
        if children.len() == 4 {
            leaves.extend(children);
        } else {
            // Culling only some children would expose a square hole where the
            // parent used to cover the globe edge.
            leaves.push((face, u, v, span, 0.0));
        }
    }
    let selected: Vec<_> = leaves
        .into_iter()
        .map(|(f, u, v, s, _)| (f, u, v, s))
        .collect();
    cache.refined.clear();
    for &(face, u, v, span) in &selected {
        let mut parent = u32::from(span) * 2;
        while parent <= u32::from(count) {
            let s = parent as u16;
            cache.refined.insert((face, u / s * s, v / s * s, s));
            parent *= 2;
        }
    }
    selected
        .into_iter()
        .take_while(|_| !cancelled())
        .map(|(face, u, v, span)| {
            let key = SectorKey::Planet(PlanetSectorKey::new(face, u, v).expect("valid leaf"));
            let lod = 16 + span.trailing_zeros() as u8;
            let geometry = cache.get(key, lod, || {
                let mut geometry = minimium_procgen::generate_planet_render_block_with_geography(
                    manifest,
                    face,
                    u,
                    v,
                    span,
                    geography.clone(),
                );
                if span == 1 {
                    if let Some((_, g)) = collision.iter().find(|(k, _)| *k == key) {
                        geometry.boxes = g.boxes.clone();
                    }
                }
                geometry
            });
            let geometry = cache.with_boxes(
                key,
                lod,
                geometry,
                if span == 1 {
                    collision
                        .iter()
                        .find(|(k, _)| *k == key)
                        .map(|(_, g)| g.boxes.as_slice())
                } else {
                    None
                },
            );
            (key, lod, geometry)
        })
        .collect()
}
