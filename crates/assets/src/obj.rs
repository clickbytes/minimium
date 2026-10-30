use super::map::{
    MAX_MAP_TOTAL_MESH_BYTES, MAX_MESH_BYTES, MAX_MESH_VERTICES, MAX_OBJ_FACE_VERTICES,
};
use minimium_core::world::triangle_area_squared;
use minimium_core::world::{MapTriangle, MAX_MAP_COORDINATE, MAX_MAP_TRIANGLES};
use std::fs::File;
use std::io::Read;
use std::path::{Component, Path};

pub(super) fn parse_mesh_asset_path(
    value: &str,
    line_number: usize,
) -> Result<std::path::PathBuf, String> {
    let path = Path::new(value);
    if value.is_empty()
        || path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(format!(
            "line {line_number}: mesh path must be relative and cannot contain `.` or `..` components"
        ));
    }
    Ok(path.to_path_buf())
}

#[derive(Clone, Copy)]
pub(super) struct ObjMeshOptions {
    pub(super) translation: [f32; 3],
    pub(super) scale: [f32; 3],
    pub(super) color: [f32; 3],
    pub(super) solid: bool,
}

pub(super) fn load_obj_triangles(
    base_dir: &Path,
    asset_path: &Path,
    options: ObjMeshOptions,
    remaining_map_bytes: usize,
    triangles: &mut Vec<MapTriangle>,
) -> Result<usize, String> {
    let ObjMeshOptions {
        translation,
        scale,
        color,
        solid,
    } = options;
    let canonical_base = std::fs::canonicalize(base_dir)
        .map_err(|error| format!("cannot resolve map directory: {error}"))?;
    let joined_path = canonical_base.join(asset_path);
    let canonical_asset = std::fs::canonicalize(&joined_path).map_err(|error| {
        format!(
            "cannot resolve mesh asset `{}`: {error}",
            asset_path.display()
        )
    })?;
    if !canonical_asset.starts_with(&canonical_base) {
        return Err(format!(
            "mesh asset `{}` resolves outside the map directory",
            asset_path.display()
        ));
    }

    let mut source = String::new();
    let byte_limit = MAX_MESH_BYTES.min(remaining_map_bytes);
    if byte_limit == 0 {
        return Err(format!(
            "map mesh assets exceed {MAX_MAP_TOTAL_MESH_BYTES} bytes"
        ));
    }
    File::open(&canonical_asset)
        .and_then(|file| {
            file.take((byte_limit + 1) as u64)
                .read_to_string(&mut source)
        })
        .map_err(|error| format!("cannot read mesh asset `{}`: {error}", asset_path.display()))?;
    if source.len() > byte_limit {
        if remaining_map_bytes < MAX_MESH_BYTES {
            return Err(format!(
                "map mesh assets exceed {MAX_MAP_TOTAL_MESH_BYTES} bytes"
            ));
        }
        return Err(format!(
            "mesh asset `{}` exceeds {MAX_MESH_BYTES} bytes",
            asset_path.display()
        ));
    }

    let mut vertices = Vec::<[f32; 3]>::with_capacity(1024);
    let mut added_triangles = 0usize;
    for (index, line) in source.lines().enumerate() {
        let line_number = index + 1;
        let content = line.split('#').next().unwrap_or("").trim();
        if content.is_empty() {
            continue;
        }
        let fields: Vec<_> = content.split_whitespace().collect();
        match fields[0] {
            "v" if fields.len() >= 4 => {
                if vertices.len() >= MAX_MESH_VERTICES {
                    return Err(format!(
                        "mesh asset `{}` exceeds {MAX_MESH_VERTICES} vertices",
                        asset_path.display()
                    ));
                }
                let mut vertex = [0.0; 3];
                for axis in 0..3 {
                    vertex[axis] = fields[axis + 1].parse::<f32>().map_err(|_| {
                        format!(
                            "mesh asset `{}` line {line_number}: invalid vertex coordinate",
                            asset_path.display()
                        )
                    })?;
                    if !vertex[axis].is_finite() || vertex[axis].abs() > MAX_MAP_COORDINATE {
                        return Err(format!(
                            "mesh asset `{}` line {line_number}: vertex is outside supported range",
                            asset_path.display()
                        ));
                    }
                }
                vertices.push(vertex);
            }
            "v" => {
                return Err(format!(
                    "mesh asset `{}` line {line_number}: vertex requires three coordinates",
                    asset_path.display()
                ));
            }
            "f" if fields.len() >= 4 => {
                let face_vertex_count = fields.len() - 1;
                if face_vertex_count > MAX_OBJ_FACE_VERTICES {
                    return Err(format!(
                        "mesh asset `{}` line {line_number}: face exceeds {MAX_OBJ_FACE_VERTICES} vertices",
                        asset_path.display()
                    ));
                }
                let mut polygon = Vec::with_capacity(face_vertex_count);
                for field in fields.iter().skip(1) {
                    polygon.push(parse_obj_vertex_index(field, vertices.len()).map_err(|_| {
                        format!(
                            "mesh asset `{}` line {line_number}: invalid face vertex index",
                            asset_path.display()
                        )
                    })?);
                }
                let face_triangles =
                    triangulate_obj_face(&vertices, &polygon).map_err(|reason| {
                        format!(
                        "mesh asset `{}` line {line_number}: cannot triangulate face ({reason})",
                        asset_path.display()
                    )
                    })?;
                for face_triangle in face_triangles {
                    if triangles.len() >= MAX_MAP_TRIANGLES {
                        return Err(format!("map exceeds {MAX_MAP_TRIANGLES} triangles"));
                    }
                    let local_vertices = face_triangle.map(|index| vertices[index]);
                    let world_vertices = local_vertices.map(|vertex| {
                        [
                            vertex[0] * scale[0] + translation[0],
                            vertex[1] * scale[1] + translation[1],
                            vertex[2] * scale[2] + translation[2],
                        ]
                    });
                    if world_vertices
                        .iter()
                        .flatten()
                        .any(|value| !value.is_finite() || value.abs() > MAX_MAP_COORDINATE)
                    {
                        return Err(format!(
                            "mesh asset `{}` line {line_number}: transformed vertex is outside supported range",
                            asset_path.display()
                        ));
                    }
                    if triangle_area_squared(world_vertices) <= 1.0e-12 {
                        return Err(format!(
                            "mesh asset `{}` line {line_number}: face creates a degenerate triangle",
                            asset_path.display()
                        ));
                    }
                    triangles.push(MapTriangle {
                        vertices: world_vertices,
                        precise_vertices: None,
                        color,
                        solid,
                    });
                    added_triangles += 1;
                }
            }
            "f" => {
                return Err(format!(
                    "mesh asset `{}` line {line_number}: face requires at least three vertices",
                    asset_path.display()
                ));
            }
            "vt" | "vn" | "o" | "g" | "s" | "usemtl" | "mtllib" | "l" | "p" => {}
            _ => {
                return Err(format!(
                    "mesh asset `{}` line {line_number}: unsupported OBJ record `{}`",
                    asset_path.display(),
                    fields[0]
                ));
            }
        }
    }
    if added_triangles == 0 {
        return Err(format!(
            "mesh asset `{}` contains no faces",
            asset_path.display()
        ));
    }
    Ok(source.len())
}

pub(super) fn parse_obj_vertex_index(field: &str, vertex_count: usize) -> Result<usize, ()> {
    let raw = field
        .split('/')
        .next()
        .unwrap_or("")
        .parse::<i64>()
        .map_err(|_| ())?;
    if raw == 0 {
        return Err(());
    }
    let resolved = if raw > 0 {
        raw - 1
    } else {
        i64::try_from(vertex_count).map_err(|_| ())? + raw
    };
    usize::try_from(resolved)
        .ok()
        .filter(|index| *index < vertex_count)
        .ok_or(())
}

pub(super) fn triangulate_obj_face(
    vertices: &[[f32; 3]],
    face: &[usize],
) -> Result<Vec<[usize; 3]>, &'static str> {
    if face.len() < 3 {
        return Err("face has fewer than three vertices");
    }

    let mut normal = [0.0f64; 3];
    for index in 0..face.len() {
        let current = vertices[face[index]].map(f64::from);
        let next = vertices[face[(index + 1) % face.len()]].map(f64::from);
        normal[0] += (current[1] - next[1]) * (current[2] + next[2]);
        normal[1] += (current[2] - next[2]) * (current[0] + next[0]);
        normal[2] += (current[0] - next[0]) * (current[1] + next[1]);
    }
    let normal_length =
        (normal[0] * normal[0] + normal[1] * normal[1] + normal[2] * normal[2]).sqrt();
    if !normal_length.is_finite() || normal_length <= 1.0e-12 {
        return Err("face has no stable plane");
    }
    let unit_normal = normal.map(|value| value / normal_length);
    let anchor = vertices[face[0]].map(f64::from);
    let mut extent: f64 = 0.0;
    for index in face {
        let vertex = vertices[*index].map(f64::from);
        let delta = [
            vertex[0] - anchor[0],
            vertex[1] - anchor[1],
            vertex[2] - anchor[2],
        ];
        extent = extent
            .max(delta[0].abs())
            .max(delta[1].abs())
            .max(delta[2].abs());
        let distance =
            delta[0] * unit_normal[0] + delta[1] * unit_normal[1] + delta[2] * unit_normal[2];
        if distance.abs() > 1.0e-4 * extent.max(1.0) {
            return Err("face is not planar");
        }
    }

    let drop_axis = if unit_normal[0].abs() >= unit_normal[1].abs()
        && unit_normal[0].abs() >= unit_normal[2].abs()
    {
        0
    } else if unit_normal[1].abs() >= unit_normal[2].abs() {
        1
    } else {
        2
    };
    let projected: Vec<[f64; 2]> = face
        .iter()
        .map(|index| {
            let vertex = vertices[*index].map(f64::from);
            match drop_axis {
                0 => [vertex[1], vertex[2]],
                1 => [vertex[0], vertex[2]],
                _ => [vertex[0], vertex[1]],
            }
        })
        .collect();
    let scale = extent.max(1.0);
    let epsilon = scale * scale * 1.0e-12;

    for first in 0..projected.len() {
        for second in first + 1..projected.len() {
            let delta_x = projected[first][0] - projected[second][0];
            let delta_y = projected[first][1] - projected[second][1];
            if delta_x * delta_x + delta_y * delta_y <= epsilon {
                return Err("face contains duplicate vertices");
            }
        }
    }

    let signed_area: f64 = (0..projected.len())
        .map(|index| {
            let current = projected[index];
            let next = projected[(index + 1) % projected.len()];
            current[0] * next[1] - next[0] * current[1]
        })
        .sum();
    if signed_area.abs() <= epsilon {
        return Err("face has zero area");
    }
    let orientation = signed_area.signum();

    let mut active: Vec<usize> = (0..face.len()).collect();
    let mut removed_collinear = true;
    while removed_collinear && active.len() > 3 {
        removed_collinear = false;
        for cursor in 0..active.len() {
            let previous = active[(cursor + active.len() - 1) % active.len()];
            let current = active[cursor];
            let next = active[(cursor + 1) % active.len()];
            let incoming = [
                projected[current][0] - projected[previous][0],
                projected[current][1] - projected[previous][1],
            ];
            let outgoing = [
                projected[next][0] - projected[current][0],
                projected[next][1] - projected[current][1],
            ];
            let turn = incoming[0] * outgoing[1] - incoming[1] * outgoing[0];
            if turn.abs() <= epsilon {
                let direction = incoming[0] * outgoing[0] + incoming[1] * outgoing[1];
                if direction < -epsilon {
                    return Err("face boundary doubles back");
                }
                active.remove(cursor);
                removed_collinear = true;
                break;
            }
        }
    }

    let mut result = Vec::with_capacity(active.len().saturating_sub(2));
    while active.len() > 3 {
        let mut clipped = false;
        for cursor in 0..active.len() {
            let previous = active[(cursor + active.len() - 1) % active.len()];
            let current = active[cursor];
            let next = active[(cursor + 1) % active.len()];
            let a = projected[previous];
            let b = projected[current];
            let c = projected[next];
            let turn = (b[0] - a[0]) * (c[1] - b[1]) - (b[1] - a[1]) * (c[0] - b[0]);
            if turn * orientation <= epsilon {
                continue;
            }

            let contains_vertex = active.iter().copied().any(|candidate| {
                if candidate == previous || candidate == current || candidate == next {
                    return false;
                }
                let point = projected[candidate];
                let ab = (b[0] - a[0]) * (point[1] - a[1]) - (b[1] - a[1]) * (point[0] - a[0]);
                let bc = (c[0] - b[0]) * (point[1] - b[1]) - (c[1] - b[1]) * (point[0] - b[0]);
                let ca = (a[0] - c[0]) * (point[1] - c[1]) - (a[1] - c[1]) * (point[0] - c[0]);
                ab * orientation >= -epsilon
                    && bc * orientation >= -epsilon
                    && ca * orientation >= -epsilon
            });
            if contains_vertex {
                continue;
            }

            result.push([face[previous], face[current], face[next]]);
            active.remove(cursor);
            clipped = true;
            break;
        }
        if !clipped {
            return Err("face is self-intersecting or numerically unstable");
        }
    }
    let last = [face[active[0]], face[active[1]], face[active[2]]];
    let a = projected[active[0]];
    let b = projected[active[1]];
    let c = projected[active[2]];
    let final_turn = (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]);
    if final_turn * orientation <= epsilon {
        return Err("remaining polygon is degenerate");
    }
    result.push(last);
    Ok(result)
}

pub(super) fn parse_map_vec3(
    fields: &[&str],
    line_number: usize,
    name: &str,
) -> Result<[f32; 3], String> {
    let mut values = [0.0; 3];
    for (index, field) in fields.iter().enumerate() {
        values[index] = field
            .parse::<f32>()
            .map_err(|_| format!("line {line_number}: invalid {name}"))?;
        if !values[index].is_finite() || values[index].abs() > MAX_MAP_COORDINATE {
            return Err(format!(
                "line {line_number}: {name} is outside supported range"
            ));
        }
    }
    Ok(values)
}
