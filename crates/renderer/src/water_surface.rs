//! CPU tessellation hook for local VOF interface planes supplied by the host.

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WaterInterfacePlane {
    pub cell_center_m: [f64; 3],
    pub normal: [f64; 3],
    /// Signed plane offset from the cell center along `normal`.
    pub offset_m: f64,
    pub flow_m_s: [f32; 3],
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WaterSurfaceVertex {
    pub position_m: [f64; 3],
    pub normal: [f32; 3],
    pub flow_m_s: [f32; 3],
}

/// Clips a reconstructed plane against its 0.5 m cell and emits a triangle fan.
/// Hosts supply the same plane reconstruction used by the fluid transport.
pub fn plane_triangles(plane: WaterInterfacePlane) -> Vec<WaterSurfaceVertex> {
    if plane
        .cell_center_m
        .iter()
        .chain(plane.normal.iter())
        .any(|v| !v.is_finite())
        || plane.flow_m_s.iter().any(|v| !v.is_finite())
        || !plane.offset_m.is_finite()
    {
        return Vec::new();
    }
    let magnitude = plane.normal.iter().map(|v| v * v).sum::<f64>().sqrt();
    if magnitude <= 1.0e-9 {
        return Vec::new();
    }
    let normal = plane.normal.map(|v| v / magnitude);
    let corners = cube_corners();
    let edges = cube_edges();
    let mut polygon = Vec::<[f64; 3]>::new();
    for (a, b) in edges {
        let da = distance(corners[a], normal, plane.offset_m);
        let db = distance(corners[b], normal, plane.offset_m);
        if da.abs() < 1.0e-9 {
            push_unique(&mut polygon, corners[a]);
        }
        if (da < 0.0 && db > 0.0) || (da > 0.0 && db < 0.0) {
            let t = da / (da - db);
            push_unique(
                &mut polygon,
                std::array::from_fn(|axis| {
                    corners[a][axis] + t * (corners[b][axis] - corners[a][axis])
                }),
            );
        }
    }
    if polygon.len() < 3 {
        return Vec::new();
    }
    let center: [f64; 3] = std::array::from_fn(|axis| {
        polygon.iter().map(|p| p[axis]).sum::<f64>() / polygon.len() as f64
    });
    let tangent = normalize(cross(normal, [0.0, 1.0, 0.0])).unwrap_or([1.0, 0.0, 0.0]);
    let bitangent = cross(normal, tangent);
    polygon.sort_by(|a, b| {
        let angle = |p: [f64; 3]| {
            let d = std::array::from_fn::<_, 3, _>(|axis| p[axis] - center[axis]);
            dot(d, bitangent).atan2(dot(d, tangent))
        };
        angle(*a).total_cmp(&angle(*b))
    });
    let flow = plane.flow_m_s;
    let vertex = |point: [f64; 3]| WaterSurfaceVertex {
        position_m: std::array::from_fn(|axis| plane.cell_center_m[axis] + point[axis]),
        normal: normal.map(|v| v as f32),
        flow_m_s: flow,
    };
    let mut triangles = Vec::with_capacity((polygon.len() - 2) * 3);
    for index in 1..polygon.len() - 1 {
        triangles.extend([
            vertex(polygon[0]),
            vertex(polygon[index]),
            vertex(polygon[index + 1]),
        ]);
    }
    triangles
}

fn distance(point: [f64; 3], normal: [f64; 3], offset: f64) -> f64 {
    dot(point, normal) - offset
}
fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn normalize(v: [f64; 3]) -> Option<[f64; 3]> {
    let n = dot(v, v).sqrt();
    (n > 1.0e-9).then(|| v.map(|x| x / n))
}
fn push_unique(out: &mut Vec<[f64; 3]>, point: [f64; 3]) {
    if out.iter().all(|p| {
        p.iter()
            .zip(point)
            .map(|(a, b)| (a - b).powi(2))
            .sum::<f64>()
            > 1.0e-16
    }) {
        out.push(point);
    }
}

fn cube_corners() -> [[f64; 3]; 8] {
    std::array::from_fn(|i| {
        [
            if i & 1 == 0 { -0.25 } else { 0.25 },
            if i & 2 == 0 { -0.25 } else { 0.25 },
            if i & 4 == 0 { -0.25 } else { 0.25 },
        ]
    })
}

fn cube_edges() -> [(usize, usize); 12] {
    [
        (0, 1),
        (2, 3),
        (4, 5),
        (6, 7),
        (0, 2),
        (1, 3),
        (4, 6),
        (5, 7),
        (0, 4),
        (1, 5),
        (2, 6),
        (3, 7),
    ]
}
