use minimium_core::world::{
    PlanetFace, PlanetSectorKey, SectorKey, WorldManifest, PLANET_SAMPLE_COORDINATE_SCALE,
};
use minimium_procgen::{planet_surface_for, PlanetTerrain, SectorGeometry};
use std::collections::{HashSet, VecDeque};
use std::sync::Arc;

const MAX_VISIBLE_SECTORS: usize = 512;

pub(super) fn render_patches(
    manifest: WorldManifest,
    position: [f32; 3],
    collision: &[(SectorKey, Arc<SectorGeometry>)],
) -> Vec<(SectorKey, u8, Arc<SectorGeometry>)> {
    if manifest.planet().is_some() {
        return dense_render_patches(manifest, position, collision);
    }
    let mut terrain = PlanetTerrain::new(manifest.generation(), planet_surface_for(manifest))
        .expect("planet manifest is compatible with the surface");
    if let Some(definition) = manifest.planet() {
        terrain = terrain.with_definition(definition);
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
        let mut patch = minimium_procgen::generate_planet_render_patch(manifest, key, subdivisions);
        if let Some((_, collision_geometry)) =
            collision.iter().find(|(candidate, _)| *candidate == sector)
        {
            patch.boxes = collision_geometry.boxes.clone();
        }
        patches.push((sector, subdivisions as u8, Arc::new(patch)));
        queue.extend(key.neighbors_with_count(count));
    }
    patches
}

fn dense_render_patches(
    manifest: WorldManifest,
    position: [f32; 3],
    collision: &[(SectorKey, Arc<SectorGeometry>)],
) -> Vec<(SectorKey, u8, Arc<SectorGeometry>)> {
    let count = minimium_procgen::planet_sector_count(manifest);
    let radius = minimium_procgen::planet_surface_for(manifest).radius_mm() as f64 / 1000.0;
    let camera = position.map(f64::from);
    let camera_radius = camera.iter().map(|value| value * value).sum::<f64>().sqrt();
    let horizon = (camera_radius * camera_radius - radius * radius)
        .max(0.0)
        .sqrt()
        .max(220.0);
    let mut terrain = PlanetTerrain::new(
        manifest.generation(),
        minimium_procgen::planet_surface_for(manifest),
    )
    .expect("planet manifest is compatible with the surface");
    if let Some(definition) = manifest.planet() {
        terrain = terrain.with_definition(definition);
    }
    let mut stack: Vec<_> = PlanetFace::ALL
        .iter()
        .map(|face| (*face, 0_u16, 0_u16, count))
        .collect();
    let mut patches = Vec::new();
    while let Some((face, u, v, span)) = stack.pop() {
        let grid_u = u32::from(u) * 2 + u32::from(span);
        let grid_v = u32::from(v) * 2 + u32::from(span);
        let scale = PLANET_SAMPLE_COORDINATE_SCALE as i64;
        let cube_u = (-scale + scale * i64::from(grid_u) / i64::from(count)) as i32;
        let cube_v = (-scale + scale * i64::from(grid_v) / i64::from(count)) as i32;
        let sample = face
            .cube_sample(cube_u, cube_v)
            .expect("quadtree midpoint lies on face");
        let center = terrain
            .sample(sample)
            .position_mm
            .coordinates()
            .map(|x| x as f64 / 1000.0);
        let distance = center
            .iter()
            .zip(camera)
            .map(|(a, b)| (a - b) * (a - b))
            .sum::<f64>()
            .sqrt();
        let bound = radius * 3.0 * f64::from(span) / f64::from(count);
        if distance - bound > horizon + 100.0 {
            continue;
        }
        let closest = (distance - bound).max(0.0);
        let desired = if closest < 100.0 {
            1
        } else if closest < 500.0 {
            2
        } else if closest < 1_000.0 {
            4
        } else {
            ((closest / 250.0).ceil() as u32)
                .next_power_of_two()
                .clamp(8, 2048)
        };
        if u32::from(span) > desired {
            let half = span / 2;
            for dy in [0, half] {
                for dx in [0, half] {
                    stack.push((face, u + dx, v + dy, half));
                }
            }
            continue;
        }
        let key =
            SectorKey::Planet(PlanetSectorKey::new(face, u, v).expect("quadtree key is valid"));
        let lod = 16 + span.trailing_zeros() as u8;
        let mut geometry =
            minimium_procgen::generate_planet_render_block(manifest, face, u, v, span);
        if span == 1 {
            if let Some((_, collision_geometry)) =
                collision.iter().find(|(candidate, _)| *candidate == key)
            {
                geometry.boxes = collision_geometry.boxes.clone();
            }
        }
        patches.push((key, lod, Arc::new(geometry)));
    }
    patches
}
