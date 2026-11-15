//! Complementary land/water polygons in world millimetres, before f32 upload.
//! GPU depth still handles occlusion, but no longer chooses the shoreline between
//! overlapping, nearly coplanar terrain and sea facets.
use minimium_procgen::SectorGeometry;
use minimium_world::world::MapTriangle;

#[derive(Clone, Copy)]
struct Corner {
    ground: [i64; 3],
    sea: [i64; 3],
    height: f64,
}

pub(crate) fn partition(
    ground: &SectorGeometry,
    sea: &SectorGeometry,
) -> (SectorGeometry, SectorGeometry) {
    // Paired conformance supplies corresponding triangles, including cube seams.
    if ground.triangles.len() != sea.triangles.len() {
        return (ground.clone(), sea.clone());
    }
    let mut land = SectorGeometry {
        boxes: ground.boxes.clone(),
        object_triangles: ground.object_triangles.clone(),
        triangles: Vec::with_capacity(ground.triangles.len()),
    };
    let mut water = SectorGeometry {
        boxes: Vec::new(),
        object_triangles: Vec::new(),
        triangles: Vec::with_capacity(sea.triangles.len()),
    };
    for (g, s) in ground.triangles.iter().zip(&sea.triangles) {
        let (Some(gp), Some(sp)) = (g.precise_vertices, s.precise_vertices) else {
            land.triangles.push(*g);
            water.triangles.push(*s);
            continue;
        };
        let corners = std::array::from_fn(|i| {
            let sea = glam::DVec3::from_array(sp[i].map(|x| x as f64));
            let offset =
                glam::DVec3::from_array(std::array::from_fn(|a| (gp[i][a] - sp[i][a]) as f64));
            Corner {
                ground: gp[i],
                sea: sp[i],
                // A vertex-local sign is identical in every adjacent triangle.
                // It avoids both subtracting large radii and triangle-normal drift.
                height: offset.dot(sea.normalize_or_zero()),
            }
        });
        clip(corners, false, *g, &mut land.triangles);
        clip(corners, true, *s, &mut water.triangles);
    }
    (land, water)
}

fn clip(corners: [Corner; 3], water: bool, template: MapTriangle, out: &mut Vec<MapTriangle>) {
    let mut polygon = [[0_i64; 3]; 4];
    let mut count = 0;
    for i in 0..3 {
        let a = corners[i];
        let b = corners[(i + 1) % 3];
        let inside = |c: Corner| (c.height < 0.0) == water;
        if inside(a) {
            polygon[count] = if water || a.height == 0.0 {
                a.sea
            } else {
                a.ground
            };
            count += 1;
        }
        if inside(a) != inside(b) {
            polygon[count] = crossing(a, b);
            count += 1;
        }
    }
    for i in 1..count.saturating_sub(1) {
        let precise = [polygon[0], polygon[i], polygon[i + 1]];
        let a = glam::DVec3::from_array(std::array::from_fn(|axis| {
            (precise[1][axis] - precise[0][axis]) as f64
        }));
        let b = glam::DVec3::from_array(std::array::from_fn(|axis| {
            (precise[2][axis] - precise[0][axis]) as f64
        }));
        if a.cross(b).length_squared() < 1.0 {
            continue;
        }
        out.push(MapTriangle {
            vertices: precise.map(|p| p.map(|v| v as f32 / 1000.0)),
            precise_vertices: Some(precise),
            ..template
        });
    }
}

fn crossing(mut a: Corner, mut b: Corner) -> [i64; 3] {
    // Canonical endpoint order gives both polygons and neighboring triangles
    // exactly the same rounded intersection, regardless of their winding.
    if a.sea > b.sea {
        std::mem::swap(&mut a, &mut b);
    }
    let t = (a.height / (a.height - b.height)).clamp(0.0, 1.0);
    std::array::from_fn(|i| (a.sea[i] as f64 + (b.sea[i] - a.sea[i]) as f64 * t).round() as i64)
}
