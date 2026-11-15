use minimium_procgen::{planet_surface_for, PlanetTerrain, SectorGeometry};
use minimium_world::world::{
    PlanetFace, PlanetSectorKey, SectorKey, WorldManifest, PLANET_SAMPLE_COORDINATE_SCALE,
};
use std::collections::{HashSet, VecDeque};
use std::sync::Arc;

const MAX_VISIBLE_SECTORS: usize = 512;

pub fn render_patches_for_view(
    world: minimium_world::world::WorldInstanceId,
    manifest: WorldManifest,
    view: crate::terrain_view::TerrainView,
    collision: &[(SectorKey, Arc<SectorGeometry>)],
    geography: minimium_procgen::TerrainInputs,
    cache: &mut crate::render_patch_cache::RenderPatchCache,
    cancelled: &dyn Fn() -> bool,
) -> Result<Vec<(SectorKey, u8, Arc<SectorGeometry>)>, minimium_procgen::ExcavationGeometryError> {
    cache.set_context(world, manifest, geography.material_profile);
    let position = view.position;
    if manifest.planet().is_some() {
        return dense_render_patches(manifest, view, collision, geography, cache, cancelled);
    }
    let terrain = PlanetTerrain::for_manifest(manifest, geography.clone())?;
    let radius = planet_surface_for(manifest).radius_mm() as f32 / 1000.0;
    let altitude_radius = position.iter().map(|x| x * x).sum::<f32>().sqrt();
    let horizon = (altitude_radius * altitude_radius - radius * radius)
        .max(0.0)
        .sqrt()
        .max(220.0);
    let count = minimium_procgen::planet_sector_count(manifest);
    let mut queue = VecDeque::from([PlanetSectorKey::from_direction_with_count(position, count)]);
    let mut visited = HashSet::new();
    let mut regions = Vec::new();
    while let Some(key) = queue.pop_front() {
        if cancelled() {
            break;
        }
        if !visited.insert(key) || regions.len() >= MAX_VISIBLE_SECTORS {
            continue;
        }
        let sample = key
            .cube_sample_with_count(8, 8, 16, count)
            .expect("sector midpoint is valid");
        let center = terrain
            .sample(sample)?
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
        regions.push(crate::render_contours::RenderRegion {
            sector: key,
            span: 1,
            lod: subdivisions as u8,
        });
        queue.extend(key.neighbors_with_count(count));
    }
    let bases = crate::render_contours::prepare(
        manifest,
        &geography,
        &regions,
        crate::render_contours::RenderMeshKind::Patch,
        cache,
    )?;
    let mut patches = Vec::with_capacity(regions.len());
    for region in regions {
        let sector = SectorKey::Planet(region.sector);
        let patch = if let Some(prepared) = bases.region(region) {
            cache.get_revision_result(
                sector,
                region.lod,
                prepared.cache_revision.clone(),
                || {
                    let contours = bases.contours(&prepared);
                    let mut geometry = minimium_procgen::integrate_volume_sector_with_contours(
                        &terrain,
                        region.sector,
                        count,
                        &prepared.geometry,
                        &contours,
                    )?;
                    if let Some((_, collision_geometry)) =
                        collision.iter().find(|(key, _)| *key == sector)
                    {
                        geometry.boxes = collision_geometry.boxes.clone();
                    }
                    Ok::<_, minimium_procgen::ExcavationGeometryError>(geometry)
                },
            )?
        } else {
            let revision = terrain.inputs().sector_version(sector, 1, count);
            cache.get_revision_result(sector, region.lod, revision, || {
                minimium_procgen::generate_planet_render_patch_with_geography(
                    manifest,
                    region.sector,
                    u32::from(region.lod),
                    geography.clone(),
                )
            })?
        };
        let patch = cache.with_boxes(
            sector,
            region.lod,
            patch,
            collision
                .iter()
                .find(|(key, _)| *key == sector)
                .map(|(_, geometry)| geometry.boxes.as_slice()),
        );
        patches.push((sector, region.lod, patch));
    }
    Ok(patches)
}

fn dense_render_patches(
    manifest: WorldManifest,
    view: crate::terrain_view::TerrainView,
    collision: &[(SectorKey, Arc<SectorGeometry>)],
    geography: minimium_procgen::TerrainInputs,
    cache: &mut crate::render_patch_cache::RenderPatchCache,
    cancelled: &dyn Fn() -> bool,
) -> Result<Vec<(SectorKey, u8, Arc<SectorGeometry>)>, minimium_procgen::ExcavationGeometryError> {
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
    let terrain = PlanetTerrain::for_manifest(manifest, geography.clone())?;
    let cell_width = radius * 2.0
        / f64::from(count)
        / f64::from(minimium_procgen::PLANET_RENDER_BLOCK_SUBDIVISIONS);
    let guard = view.ground_detail_radius(radius * 2.0 / f64::from(count));
    let guard = if (camera_radius - visibility_radius).abs() <= guard {
        guard
    } else {
        0.0
    };
    let surface_eye = glam::DVec3::from_array(camera).normalize_or_zero() * visibility_radius;
    let coasts = &mut cache.coasts;
    let mut candidate = |face: PlanetFace, u: u16, v: u16, span: u16| {
        let scale = PLANET_SAMPLE_COORDINATE_SCALE as i64;
        let coordinate = |p: u16| {
            (-scale + scale * (2 * i64::from(p) + i64::from(span)) / i64::from(count)) as i32
        };
        let sample = face
            .cube_sample(coordinate(u), coordinate(v))
            .expect("face sample");
        let point = terrain.sample(sample)?;
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
            return Ok(None);
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
            score *= view.surface_priority(coasts.classify(&terrain, (face, u, v, span), count)?);
        }
        if guard > 0.0 && span > 1 {
            let surface_center = glam::DVec3::from_array(sample.coordinates().map(f64::from))
                .normalize()
                * visibility_radius;
            let separation = surface_center.distance(surface_eye);
            let patch_bound = radius * 1.5 * f64::from(span) / f64::from(count);
            if separation <= guard + patch_bound {
                // Resolve nearby shores before distant terrain competes for leaves.
                // The guard extends past the player, so walking across a sector
                // boundary does not change the lattice beneath a nearby pond.
                score = score.max(1e6 / (1.0 + separation / guard));
            }
        }
        Ok::<_, minimium_procgen::GenerationError>(Some((face, u, v, span, score)))
    };
    let mut leaves = Vec::new();
    for face in PlanetFace::ALL {
        if let Some(leaf) = candidate(face, 0, 0, count)? {
            leaves.push(leaf);
        }
    }
    // Refine the most visible error first. Keep every parent until all of its
    // children fit, so the budget cannot create holes or truncate coverage.
    while leaves.len() + 3 <= view.patch_budget() {
        if cancelled() {
            return Ok(Vec::new());
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
            .max_by(|(_, a), (_, b)| {
                let priority = |n: &(PlanetFace, u16, u16, u16, f64)| {
                    n.4 * if cache.refined.contains(&(n.0, n.1, n.2, n.3)) {
                        1.25
                    } else {
                        1.0
                    }
                };
                priority(a).total_cmp(&priority(b))
            })
            .map(|(i, _)| i);
        let Some(index) = next else {
            break;
        };
        let (face, u, v, span, _) = leaves.swap_remove(index);
        let half = span / 2;
        let mut children = Vec::with_capacity(4);
        for dy in [0, half] {
            for dx in [0, half] {
                if let Some(child) = candidate(face, u + dx, v + dy, half)? {
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
    let regions: Vec<_> = selected
        .into_iter()
        .map(|(face, u, v, span)| crate::render_contours::RenderRegion {
            sector: PlanetSectorKey::new(face, u, v).expect("valid leaf"),
            span,
            lod: 16 + span.trailing_zeros() as u8,
        })
        .collect();
    let bases = crate::render_contours::prepare(
        manifest,
        &geography,
        &regions,
        crate::render_contours::RenderMeshKind::Block,
        cache,
    )?;
    let mut output = Vec::with_capacity(regions.len());
    for region in regions {
        if cancelled() {
            break;
        }
        let key = SectorKey::Planet(region.sector);
        let geometry = if let Some(prepared) = bases.region(region) {
            cache.get_revision_result(key, region.lod, prepared.cache_revision.clone(), || {
                let contours = bases.contours(&prepared);
                let (u, v) = region.sector.coordinates();
                let mut geometry = minimium_procgen::integrate_volume_region_with_contours(
                    &terrain,
                    region.sector.face(),
                    u,
                    v,
                    region.span,
                    count,
                    &prepared.geometry,
                    &contours,
                )?;
                if region.span == 1 {
                    if let Some((_, collision_geometry)) =
                        collision.iter().find(|(candidate, _)| *candidate == key)
                    {
                        geometry.boxes = collision_geometry.boxes.clone();
                    }
                }
                Ok::<_, minimium_procgen::ExcavationGeometryError>(geometry)
            })?
        } else {
            let revision = terrain.inputs().sector_version(key, region.span, count);
            cache.get_revision_result(key, region.lod, revision, || {
                minimium_procgen::generate_planet_render_block_with_geography(
                    manifest,
                    region.sector.face(),
                    region.sector.coordinates().0,
                    region.sector.coordinates().1,
                    region.span,
                    geography.clone(),
                )
            })?
        };
        let geometry = cache.with_boxes(
            key,
            region.lod,
            geometry,
            if region.span == 1 {
                collision
                    .iter()
                    .find(|(candidate, _)| *candidate == key)
                    .map(|(_, geometry)| geometry.boxes.as_slice())
            } else {
                None
            },
        );
        output.push((key, region.lod, geometry));
    }
    Ok(output)
}
