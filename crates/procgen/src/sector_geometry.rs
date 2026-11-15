use crate::sector_neighborhood::DUNGEON_SECTOR_EDGE;
use minimium_world::world::{
    MapBox, MapTriangle, PlanetFace, PlanetSectorKey, SectorKey, WorldMap, WorldTopology,
    PLANET_SECTORS_PER_FACE,
};

#[derive(Clone, Default)]
pub struct SectorGeometry {
    /// Solid object meshes, kept separate from sampled terrain materials.
    pub object_triangles: Vec<MapTriangle>,
    pub boxes: Vec<MapBox>,
    pub triangles: Vec<MapTriangle>,
}

/// Slices render and collision geometry using the shared planet or dungeon
/// sector address contract.
pub fn slice_sector_geometry(
    map: &WorldMap,
    topology: WorldTopology,
    key: SectorKey,
) -> SectorGeometry {
    slice_sector_geometry_with_count(map, topology, key, PLANET_SECTORS_PER_FACE)
}

fn slice_sector_geometry_with_count(
    map: &WorldMap,
    topology: WorldTopology,
    key: SectorKey,
    count: u16,
) -> SectorGeometry {
    let mut geometry = SectorGeometry::default();
    for map_box in map.boxes() {
        let included = match (topology, key) {
            (WorldTopology::Planet, SectorKey::Planet(sector)) => {
                planet_box_touches(*map_box, sector, count)
            }
            (WorldTopology::Dungeon, SectorKey::Dungeon(sector)) => {
                dungeon_box_overlaps(*map_box, sector.coordinates())
            }
            _ => false,
        };
        if included {
            geometry.boxes.push(*map_box);
        }
    }
    for triangle in map.triangles() {
        let included = match (topology, key) {
            (WorldTopology::Planet, SectorKey::Planet(sector)) => {
                planet_triangle_touches(*triangle, sector, count)
            }
            (WorldTopology::Dungeon, SectorKey::Dungeon(sector)) => {
                dungeon_triangle_touches(*triangle, sector.coordinates())
            }
            _ => false,
        };
        if included {
            geometry.triangles.push(*triangle);
        }
    }
    geometry
}

/// Shared server/client source for the current manifest's sector geometry.
pub fn materialize_world_sector(
    map: &WorldMap,
    manifest: minimium_world::world::WorldManifest,
    key: SectorKey,
) -> Result<SectorGeometry, crate::ExcavationGeometryError> {
    materialize_world_sector_with_geography(map, manifest, key, None)
}

pub fn materialize_world_sector_with_geography(
    map: &WorldMap,
    manifest: minimium_world::world::WorldManifest,
    key: SectorKey,
    geography: impl Into<crate::TerrainInputs>,
) -> Result<SectorGeometry, crate::ExcavationGeometryError> {
    let inputs: crate::TerrainInputs = geography.into();
    if inputs.volumes.is_some()
        && !inputs
            .material_profile
            .is_some_and(|profile| profile.matches(manifest, map.fingerprint()))
    {
        return Err(crate::ExcavationGeometryError::UnavailableSurface);
    }
    let mut geometry = slice_sector_geometry_with_count(
        map,
        manifest.topology(),
        key,
        crate::planet_sector_count(manifest),
    );
    if let (WorldTopology::Planet, SectorKey::Planet(planet)) = (manifest.topology(), key) {
        let generated =
            crate::generate_planet_sector_with_geography(manifest, planet, inputs.clone())?;
        geometry.triangles.extend(generated.triangles);
        geometry.object_triangles.extend(generated.object_triangles);
        if let Some(store) = inputs.volumes.as_deref() {
            let terrain = crate::PlanetTerrain::for_manifest(manifest, inputs.clone())?;
            let count = crate::planet_sector_count(manifest);
            let (u, v) = planet.coordinates();
            let contour_sectors =
                crate::volume_contour_sectors(store, planet.face(), u, v, 1, count)?;
            if contour_sectors.is_empty() {
                return Ok(geometry);
            }
            let mut contour_source = Vec::new();
            for adjacent in contour_sectors {
                let context = crate::generate_planet_sector_with_geography(
                    manifest,
                    adjacent,
                    inputs.clone(),
                )?;
                contour_source.extend(context.triangles);
            }
            geometry = crate::integrate_volume_sector_with_contours(
                &terrain,
                planet,
                count,
                &geometry,
                &contour_source,
            )?;
        }
    }
    Ok(geometry)
}

fn planet_box_touches(map_box: MapBox, sector: PlanetSectorKey, count: u16) -> bool {
    let half = map_box.size.map(|value| value.abs() * 0.5);
    let minimum = std::array::from_fn::<_, 3, _>(|axis| map_box.center[axis] - half[axis]);
    let maximum = std::array::from_fn::<_, 3, _>(|axis| map_box.center[axis] + half[axis]);
    if (0..3).all(|axis| minimum[axis] <= 0.0 && maximum[axis] >= 0.0) {
        // Every ray from the origin intersects a box that contains it.
        return true;
    }

    let corners: Vec<[f32; 3]> = (0..8)
        .map(|mask| {
            std::array::from_fn(|axis| {
                if mask & (1 << axis) == 0 {
                    minimum[axis]
                } else {
                    maximum[axis]
                }
            })
        })
        .collect();
    // Two triangles cover each of the six closed box faces. Clip those
    // polygons against each face-sector cone, including cross-face seams.
    const FACE_TRIANGLES: [[usize; 3]; 12] = [
        [0, 2, 6],
        [0, 6, 4],
        [1, 5, 7],
        [1, 7, 3],
        [0, 4, 5],
        [0, 5, 1],
        [2, 3, 7],
        [2, 7, 6],
        [0, 1, 3],
        [0, 3, 2],
        [4, 6, 7],
        [4, 7, 5],
    ];
    FACE_TRIANGLES.into_iter().any(|indices| {
        planet_triangle_touches(
            MapTriangle {
                vertices: indices.map(|index| corners[index]),
                precise_vertices: None,
                color: [0.0; 3],
                solid: false,
                surface: None,
            },
            sector,
            count,
        )
    })
}

fn planet_triangle_touches(triangle: MapTriangle, sector: PlanetSectorKey, count: u16) -> bool {
    let (normal, u_axis, v_axis) = planet_face_basis(sector.face());
    let axes = [
        normal,
        add_axes(normal, u_axis),
        subtract_axes(normal, u_axis),
        add_axes(normal, v_axis),
        subtract_axes(normal, v_axis),
    ];
    let mut polygon: Vec<[f64; 3]> = triangle.vertices.map(|point| point.map(f64::from)).into();
    for axis in axes {
        polygon = clip_polygon_to_face(polygon, axis);
        if polygon.is_empty() {
            return false;
        }
    }
    let mut minimum = [f32::INFINITY; 2];
    let mut maximum = [f32::NEG_INFINITY; 2];
    for point in polygon {
        let denominator = axis_component(point, normal);
        if denominator <= 1.0e-8 {
            // The origin itself has no UV address. Other clipped vertices
            // bound the one-sided projected triangle around it.
            continue;
        }
        for (axis, basis) in [u_axis, v_axis].into_iter().enumerate() {
            let coordinate = (axis_component(point, basis) / denominator) as f32;
            minimum[axis] = minimum[axis].min(coordinate);
            maximum[axis] = maximum[axis].max(coordinate);
        }
    }
    if minimum.iter().any(|value| !value.is_finite()) {
        return false;
    }
    let (u, v) = sector.coordinates();
    overlaps_planet_interval(minimum[0], maximum[0], u, count)
        && overlaps_planet_interval(minimum[1], maximum[1], v, count)
}

fn clip_polygon_to_face(polygon: Vec<[f64; 3]>, axis: [i8; 3]) -> Vec<[f64; 3]> {
    if polygon.is_empty() {
        return polygon;
    }
    let scale = polygon
        .iter()
        .flatten()
        .map(|value| value.abs())
        .fold(1.0_f64, f64::max);
    let tolerance = 2.0 * f64::from(f32::EPSILON) * scale;
    let signed = |point: [f64; 3]| axis_component(point, axis) + tolerance;
    let mut output = Vec::with_capacity(polygon.len() + 1);
    let mut previous = *polygon.last().unwrap();
    let mut previous_distance = signed(previous);
    for current in polygon {
        let current_distance = signed(current);
        let previous_inside = previous_distance >= 0.0;
        let current_inside = current_distance >= 0.0;
        if previous_inside != current_inside {
            let fraction = previous_distance / (previous_distance - current_distance);
            output.push(std::array::from_fn(|axis| {
                previous[axis] + (current[axis] - previous[axis]) * fraction
            }));
        }
        if current_inside {
            output.push(current);
        }
        previous = current;
        previous_distance = current_distance;
    }
    output
}

fn axis_component(point: [f64; 3], axis: [i8; 3]) -> f64 {
    (0..3)
        .map(|dimension| point[dimension] * f64::from(axis[dimension]))
        .sum()
}

fn add_axes(left: [i8; 3], right: [i8; 3]) -> [i8; 3] {
    std::array::from_fn(|axis| left[axis] + right[axis])
}

fn subtract_axes(left: [i8; 3], right: [i8; 3]) -> [i8; 3] {
    std::array::from_fn(|axis| left[axis] - right[axis])
}

fn planet_face_basis(face: PlanetFace) -> ([i8; 3], [i8; 3], [i8; 3]) {
    match face {
        PlanetFace::PositiveX => ([1, 0, 0], [0, 1, 0], [0, 0, 1]),
        PlanetFace::NegativeX => ([-1, 0, 0], [0, 1, 0], [0, 0, 1]),
        PlanetFace::PositiveY => ([0, 1, 0], [1, 0, 0], [0, 0, 1]),
        PlanetFace::NegativeY => ([0, -1, 0], [1, 0, 0], [0, 0, 1]),
        PlanetFace::PositiveZ => ([0, 0, 1], [1, 0, 0], [0, 1, 0]),
        PlanetFace::NegativeZ => ([0, 0, -1], [1, 0, 0], [0, 1, 0]),
    }
}

fn overlaps_planet_interval(minimum: f32, maximum: f32, sector: u16, count: u16) -> bool {
    let count = f32::from(count);
    let start = -1.0 + 2.0 * f32::from(sector) / count;
    let end = start + 2.0 / count;
    minimum <= end && maximum >= start
}

fn dungeon_box_overlaps(map_box: MapBox, (sector_x, sector_y, sector_z): (i32, i32, i32)) -> bool {
    let half = map_box.size.map(|value| value * 0.5);
    let center = map_box.center;
    overlaps_interval(center[0] - half[0], center[0] + half[0], sector_x)
        && overlaps_interval(center[1] - half[1], center[1] + half[1], sector_y)
        && overlaps_interval(center[2] - half[2], center[2] + half[2], sector_z)
}

fn dungeon_triangle_touches(
    triangle: MapTriangle,
    (sector_x, sector_y, sector_z): (i32, i32, i32),
) -> bool {
    let minimum = std::array::from_fn::<_, 3, _>(|axis| {
        triangle
            .vertices
            .iter()
            .map(|point| point[axis])
            .fold(f32::INFINITY, f32::min)
    });
    let maximum = std::array::from_fn::<_, 3, _>(|axis| {
        triangle
            .vertices
            .iter()
            .map(|point| point[axis])
            .fold(f32::NEG_INFINITY, f32::max)
    });
    overlaps_interval(minimum[0], maximum[0], sector_x)
        && overlaps_interval(minimum[1], maximum[1], sector_y)
        && overlaps_interval(minimum[2], maximum[2], sector_z)
}

fn overlaps_interval(minimum: f32, maximum: f32, sector: i32) -> bool {
    let edge = f32::from(DUNGEON_SECTOR_EDGE);
    let start = sector as f32 * edge;
    minimum <= start + edge && maximum >= start
}
