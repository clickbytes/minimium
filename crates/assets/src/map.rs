use crate::cube_sphere::append_cube_sphere;
use crate::obj::{load_obj_triangles, parse_map_vec3, parse_mesh_asset_path, ObjMeshOptions};
use minimium_core::world::{
    triangle_area_squared, CubeSphere, MapBox, MapTriangle, WorldMap, WorldPortal, MAX_MAP_BOXES,
    MAX_MAP_TRIANGLES,
};
use std::{
    fs::File,
    io::{self, Read},
    path::Path,
};

pub(super) const MAX_MAP_MESHES: usize = 64;
pub(super) const MAX_MAP_BYTES: usize = 1024 * 1024;
pub(super) const MAX_MESH_BYTES: usize = 8 * 1024 * 1024;
pub(super) const MAX_MAP_TOTAL_MESH_BYTES: usize = 16 * 1024 * 1024;
pub(super) const MAX_MESH_VERTICES: usize = 262_144;
pub(super) const MAX_OBJ_FACE_VERTICES: usize = 128;

pub fn load_map(path: &Path) -> io::Result<WorldMap> {
    let mut input = String::new();
    File::open(path)?
        .take((MAX_MAP_BYTES + 1) as u64)
        .read_to_string(&mut input)?;
    let base_dir = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    parse_with_base(&input, Some(base_dir))
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

pub fn parse_map(source: &str) -> Result<WorldMap, String> {
    parse_with_base(source, None)
}

fn parse_with_base(source: &str, base_dir: Option<&Path>) -> Result<WorldMap, String> {
    if source.len() > MAX_MAP_BYTES {
        return Err(format!("map exceeds {MAX_MAP_BYTES} bytes"));
    }
    let mut content_lines = source.lines().enumerate().filter_map(|(index, line)| {
        let content = line.split('#').next().unwrap_or("").trim();
        (!content.is_empty()).then_some((index + 1, content))
    });
    let Some((header_line, header)) = content_lines.next() else {
        return Err("map is empty".into());
    };
    let format_version = match header {
        "MINIMIUM_MAP 1" => 1,
        "MINIMIUM_MAP 2" => 2,
        "MINIMIUM_MAP 3" => 3,
        _ => {
            return Err(format!(
                "line {header_line}: expected `MINIMIUM_MAP 1`, `MINIMIUM_MAP 2` or `MINIMIUM_MAP 3`"
            ))
        }
    };

    let mut spawn = None;
    let mut planet = None;
    let mut portals = Vec::new();
    let mut boxes = Vec::new();
    let mut triangles = Vec::new();
    let mut mesh_count = 0usize;
    let mut total_mesh_bytes = 0usize;
    for (line_number, line) in content_lines {
        let fields: Vec<_> = line.split_whitespace().collect();
        match fields.first().copied() {
            Some("spawn") if fields.len() == 4 => {
                if spawn.is_some() {
                    return Err(format!("line {line_number}: duplicate spawn"));
                }
                spawn = Some(parse_map_vec3(&fields[1..4], line_number, "spawn")?);
            }
            Some("planet") if format_version >= 3 && fields.len() == 9 => {
                if planet.is_some() {
                    return Err(format!("line {line_number}: duplicate planet"));
                }
                let center = parse_map_vec3(&fields[1..4], line_number, "planet center")?;
                let radius = fields[4]
                    .parse::<f32>()
                    .map_err(|_| format!("line {line_number}: invalid planet radius"))?;
                let divisions = fields[5]
                    .parse::<usize>()
                    .map_err(|_| format!("line {line_number}: invalid planet divisions"))?;
                let color = parse_map_vec3(&fields[6..9], line_number, "planet color")?;
                if !radius.is_finite() || !(5.0..=1000.0).contains(&radius)
                    || color.iter().any(|value| !(0.0..=1.0).contains(value))
                {
                    return Err(format!("line {line_number}: invalid planet parameters"));
                }
                let sphere = CubeSphere { center, radius };
                append_cube_sphere(&mut triangles, sphere, divisions, color)
                    .map_err(|error| format!("line {line_number}: {error}"))?;
                planet = Some(sphere);
            }
            Some("portal") if format_version >= 3 && fields.len() == 8 => {
                let at = parse_map_vec3(&fields[1..4], line_number, "portal trigger")?;
                let destination = parse_map_vec3(&fields[4..7], line_number, "portal destination")?;
                let radius = fields[7]
                    .parse::<f32>()
                    .map_err(|_| format!("line {line_number}: invalid portal radius"))?;
                if !radius.is_finite() || !(0.5..=8.0).contains(&radius) {
                    return Err(format!("line {line_number}: invalid portal radius"));
                }
                portals.push(WorldPortal { at, destination, radius });
            }
            Some("box") if fields.len() == 11 => {
                if boxes.len() >= MAX_MAP_BOXES {
                    return Err(format!(
                        "line {line_number}: map exceeds {MAX_MAP_BOXES} boxes"
                    ));
                }
                let center = parse_map_vec3(&fields[1..4], line_number, "box center")?;
                let size = parse_map_vec3(&fields[4..7], line_number, "box size")?;
                let color = parse_map_vec3(&fields[7..10], line_number, "box color")?;
                if size.iter().any(|value| *value <= 0.0) {
                    return Err(format!("line {line_number}: box sizes must be positive"));
                }
                if color.iter().any(|value| !(0.0..=1.0).contains(value)) {
                    return Err(format!("line {line_number}: box colors must be in 0..=1"));
                }
                let solid = match fields[10] {
                    "solid" => true,
                    "visual" => false,
                    _ => return Err(format!("line {line_number}: expected `solid` or `visual`")),
                };
                boxes.push(MapBox {
                    center,
                    size,
                    color,
                    solid,
                });
            }
            Some("triangle")
                if format_version >= 2 && (fields.len() == 13 || fields.len() == 14) =>
            {
                if triangles.len() >= MAX_MAP_TRIANGLES {
                    return Err(format!(
                        "line {line_number}: map exceeds {MAX_MAP_TRIANGLES} triangles"
                    ));
                }
                let vertices = [
                    parse_map_vec3(&fields[1..4], line_number, "triangle vertex")?,
                    parse_map_vec3(&fields[4..7], line_number, "triangle vertex")?,
                    parse_map_vec3(&fields[7..10], line_number, "triangle vertex")?,
                ];
                let color = parse_map_vec3(&fields[10..13], line_number, "triangle color")?;
                if color.iter().any(|value| !(0.0..=1.0).contains(value)) {
                    return Err(format!(
                        "line {line_number}: triangle colors must be in 0..=1"
                    ));
                }
                let area_squared = triangle_area_squared(vertices);
                if area_squared <= 1.0e-12 {
                    return Err(format!(
                        "line {line_number}: triangle vertices are degenerate"
                    ));
                }
                let solid = if fields.len() == 14 {
                    match fields[13] {
                        "solid" => true,
                        "visual" => false,
                        _ => {
                            return Err(format!("line {line_number}: expected `solid` or `visual`"))
                        }
                    }
                } else {
                    false
                };
                triangles.push(MapTriangle {
                    vertices,
                    color,
                    solid,
                });
            }
            Some("triangle") => {
                return Err(format!(
                        "line {line_number}: invalid `triangle` record; triangles require map version 2"
                    ));
            }
            Some("mesh") if format_version < 2 => {
                return Err(format!(
                    "line {line_number}: mesh records require map version 2"
                ));
            }
            Some("mesh") if fields.len() == 12 => {
                if mesh_count >= MAX_MAP_MESHES {
                    return Err(format!(
                        "line {line_number}: map exceeds {MAX_MAP_MESHES} mesh assets"
                    ));
                }
                let Some(base_dir) = base_dir else {
                    return Err(format!(
                        "line {line_number}: mesh records require file-based map loading"
                    ));
                };
                let asset_path = parse_mesh_asset_path(fields[1], line_number)?;
                let translation = parse_map_vec3(&fields[2..5], line_number, "mesh translation")?;
                let scale = parse_map_vec3(&fields[5..8], line_number, "mesh scale")?;
                let color = parse_map_vec3(&fields[8..11], line_number, "mesh color")?;
                if scale.iter().any(|value| *value <= 0.0) {
                    return Err(format!(
                        "line {line_number}: mesh scale components must be positive"
                    ));
                }
                if color.iter().any(|value| !(0.0..=1.0).contains(value)) {
                    return Err(format!("line {line_number}: mesh colors must be in 0..=1"));
                }
                let solid = match fields[11] {
                    "solid" => true,
                    "visual" => false,
                    _ => return Err(format!("line {line_number}: expected `solid` or `visual`")),
                };
                let remaining_mesh_bytes =
                    MAX_MAP_TOTAL_MESH_BYTES.saturating_sub(total_mesh_bytes);
                let mesh_bytes = load_obj_triangles(
                    base_dir,
                    &asset_path,
                    ObjMeshOptions {
                        translation,
                        scale,
                        color,
                        solid,
                    },
                    remaining_mesh_bytes,
                    &mut triangles,
                )
                .map_err(|error| format!("line {line_number}: {error}"))?;
                total_mesh_bytes = total_mesh_bytes.saturating_add(mesh_bytes);
                mesh_count += 1;
            }
            Some("mesh") => {
                return Err(format!(
                        "line {line_number}: invalid `mesh` record; expected `mesh PATH TX TY TZ SX SY SZ R G B solid|visual`"
                    ));
            }
            Some("spawn") | Some("box") => {
                return Err(format!(
                    "line {line_number}: invalid `{}` record",
                    fields[0]
                ));
            }
            Some(_) => return Err(format!("line {line_number}: unknown map record")),
            None => unreachable!(),
        }
    }

    let spawn = spawn.ok_or("map is missing a spawn record")?;
    if boxes.is_empty() && triangles.is_empty() {
        return Err("map must contain at least one box or triangle".into());
    }
    WorldMap::with_regions(spawn, boxes, triangles, planet, portals)
}
