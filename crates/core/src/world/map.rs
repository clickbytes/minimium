use super::collision_index::{Aabb, CollisionIndex};
use super::geometry::{intersects_player, triangle_area_squared, triangle_intersects_player};
use super::obj::{
    canonical_float_bits, load_obj_triangles, parse_map_vec3, parse_mesh_asset_path, ObjMeshOptions,
};
use std::fs::File;
use std::io::{self, Read};
use std::path::Path;

pub(super) const MAX_MAP_BOXES: usize = 4096;
pub(super) const MAX_MAP_TRIANGLES: usize = 8192;
pub(super) const MAX_MAP_MESHES: usize = 64;
pub(super) const MAX_MAP_COORDINATE: f32 = 1_000_000.0;
pub(super) const MAX_MAP_BYTES: usize = 1024 * 1024;
pub(super) const MAX_MESH_BYTES: usize = 8 * 1024 * 1024;
pub(super) const MAX_MAP_TOTAL_MESH_BYTES: usize = 16 * 1024 * 1024;
pub(super) const MAX_MESH_VERTICES: usize = 262_144;
pub(super) const MAX_OBJ_FACE_VERTICES: usize = 128;

#[derive(Clone, Copy, Debug)]
pub struct MapBox {
    pub center: [f32; 3],
    pub size: [f32; 3],
    pub color: [f32; 3],
    pub solid: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct MapTriangle {
    pub vertices: [[f32; 3]; 3],
    pub color: [f32; 3],
    pub solid: bool,
}

#[derive(Clone, Debug)]
pub struct WorldMap {
    pub spawn: [f32; 3],
    pub(super) boxes: Vec<MapBox>,
    pub(super) triangles: Vec<MapTriangle>,
    pub(super) fingerprint: u64,
}

impl WorldMap {
    pub fn load_file(path: &Path) -> io::Result<Self> {
        let mut input = String::new();
        File::open(path)?
            .take((MAX_MAP_BYTES + 1) as u64)
            .read_to_string(&mut input)?;
        let base_dir = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        Self::parse_with_base(&input, Some(base_dir))
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
    }

    pub fn parse(source: &str) -> Result<Self, String> {
        Self::parse_with_base(source, None)
    }

    fn parse_with_base(source: &str, base_dir: Option<&Path>) -> Result<Self, String> {
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
                        _ => {
                            return Err(format!("line {line_number}: expected `solid` or `visual`"))
                        }
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
                                return Err(format!(
                                    "line {line_number}: expected `solid` or `visual`"
                                ))
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
                    let translation =
                        parse_map_vec3(&fields[2..5], line_number, "mesh translation")?;
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
                        _ => {
                            return Err(format!("line {line_number}: expected `solid` or `visual`"))
                        }
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
        if boxes.iter().filter(|map_box| map_box.solid).any(|map_box| {
            intersects_player(spawn, Aabb::from_center_size(map_box.center, map_box.size))
        }) || triangles
            .iter()
            .filter(|triangle| triangle.solid)
            .any(|triangle| triangle_intersects_player(spawn, *triangle))
        {
            return Err("spawn overlaps solid map geometry".into());
        }
        let fingerprint = map_fingerprint(spawn, &boxes, &triangles);
        Ok(Self {
            spawn,
            boxes,
            triangles,
            fingerprint,
        })
    }

    pub fn boxes(&self) -> &[MapBox] {
        &self.boxes
    }

    pub fn triangles(&self) -> &[MapTriangle] {
        &self.triangles
    }

    pub fn solid_colliders(&self) -> Vec<Aabb> {
        self.boxes
            .iter()
            .filter(|map_box| map_box.solid)
            .map(|map_box| Aabb::from_center_size(map_box.center, map_box.size))
            .collect()
    }

    pub fn solid_triangles(&self) -> Vec<MapTriangle> {
        self.triangles
            .iter()
            .copied()
            .filter(|triangle| triangle.solid)
            .collect()
    }

    pub fn collision_index(&self) -> CollisionIndex {
        CollisionIndex::from_geometry(self.solid_colliders(), self.solid_triangles())
    }

    pub fn can_place_player(&self, position: [f32; 3]) -> bool {
        position.iter().all(|value| value.is_finite())
            && self
                .boxes
                .iter()
                .filter(|map_box| map_box.solid)
                .all(|map_box| {
                    !intersects_player(
                        position,
                        Aabb::from_center_size(map_box.center, map_box.size),
                    )
                })
            && self
                .triangles
                .iter()
                .filter(|triangle| triangle.solid)
                .all(|triangle| !triangle_intersects_player(position, *triangle))
    }

    pub fn fingerprint(&self) -> u64 {
        self.fingerprint
    }
}

pub(super) fn map_fingerprint(spawn: [f32; 3], boxes: &[MapBox], triangles: &[MapTriangle]) -> u64 {
    let mut hash = 0xcbf29ce484222325;
    let mut feed = |bytes: &[u8]| {
        for byte in bytes {
            hash = (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3);
        }
    };
    feed(b"MINIMIUM_MAP 1");
    for value in spawn {
        feed(&canonical_float_bits(value).to_le_bytes());
    }
    for map_box in boxes {
        for value in map_box
            .center
            .into_iter()
            .chain(map_box.size)
            .chain(map_box.color)
        {
            feed(&canonical_float_bits(value).to_le_bytes());
        }
        feed(&[u8::from(map_box.solid)]);
    }
    if !triangles.is_empty() {
        feed(b"\0TRIANGLES\0");
        feed(&(triangles.len() as u32).to_le_bytes());
        for triangle in triangles {
            for value in triangle
                .vertices
                .into_iter()
                .flatten()
                .chain(triangle.color)
            {
                feed(&canonical_float_bits(value).to_le_bytes());
            }
            feed(&[u8::from(triangle.solid)]);
        }
    }
    hash
}
