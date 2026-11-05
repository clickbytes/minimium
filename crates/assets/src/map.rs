use crate::obj::{parse_map_vec3, parse_mesh_asset_path, parse_obj_triangles, ObjMeshOptions};
use minimium_world::world::{
    triangle_area_squared, MapBox, MapTriangle, WorldMap, MAX_MAP_BOXES, MAX_MAP_TRIANGLES,
};
use std::path::Path;

pub const MAX_MAP_MESHES: usize = 64;
pub const MAX_MAP_BYTES: usize = 1024 * 1024;
pub const MAX_MESH_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_MAP_TOTAL_MESH_BYTES: usize = 16 * 1024 * 1024;
pub(super) const MAX_MESH_VERTICES: usize = 262_144;
pub(super) const MAX_OBJ_FACE_VERTICES: usize = 128;

pub fn parse_map(source: &str) -> Result<WorldMap, String> {
    parse_map_with_mesh_loader(source, |_path, _| {
        Err("mesh records require file-based map loading".into())
    })
}

pub fn parse_map_with_mesh_loader(
    source: &str,
    mut load_mesh: impl FnMut(&Path, usize) -> Result<String, String>,
) -> Result<WorldMap, String> {
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
        _ => {
            return Err(format!(
                "line {header_line}: expected `MINIMIUM_MAP 1` or `MINIMIUM_MAP 2`"
            ))
        }
    };

    let mut spawn = None;
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
                    precise_vertices: None,
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
                let mesh_source = load_mesh(&asset_path, MAX_MESH_BYTES.min(remaining_mesh_bytes))
                    .map_err(|error| format!("line {line_number}: {error}"))?;
                let mesh_bytes = parse_obj_triangles(
                    &mesh_source,
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
    WorldMap::from_parts(spawn, boxes, triangles)
}
