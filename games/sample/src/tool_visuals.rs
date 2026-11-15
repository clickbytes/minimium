//! Sample-owned camera-local tool viewmodel geometry.

use minimium_world::items::ToolKind;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ToolModelVertex {
    pub position: [f32; 3],
    /// Linear RGB, written after atmospheric composition with unit pre-exposure.
    pub color: [f32; 3],
}

#[derive(Clone, Debug, PartialEq)]
pub struct ToolModelDefinition {
    pub kind: ToolKind,
    pub vertices: Vec<ToolModelVertex>,
}

pub const TOOL_MODEL_KINDS: [ToolKind; 2] = [ToolKind::Shovel, ToolKind::Pickaxe];

/// Returns the sample's ordered model catalog in camera-local metres.
pub fn tool_model_catalog() -> [ToolModelDefinition; 2] {
    TOOL_MODEL_KINDS.map(|kind| ToolModelDefinition {
        kind,
        vertices: vertices(kind),
    })
}

/// Returns the catalog index associated with one confirmed inventory tool.
pub fn tool_model_index(kind: ToolKind) -> Option<usize> {
    TOOL_MODEL_KINDS
        .iter()
        .position(|catalog_kind| *catalog_kind == kind)
}

fn vertices(kind: ToolKind) -> Vec<ToolModelVertex> {
    let mut vertices = Vec::with_capacity(72);
    let grip = [0.34, -0.24, -0.56];
    let head = [0.035, -0.57, -0.94];
    segment_prism(
        &mut vertices,
        [0.31, -0.20, -0.53],
        [0.39, -0.29, -0.62],
        0.042,
        [0.27, 0.16, 0.10],
        [0.47, 0.31, 0.20],
    );
    segment_prism(
        &mut vertices,
        grip,
        head,
        0.018,
        [0.25, 0.105, 0.035],
        [0.39, 0.19, 0.07],
    );
    match kind {
        ToolKind::Shovel => shovel_head(&mut vertices, head),
        ToolKind::Pickaxe => pickaxe_head(&mut vertices, head),
    }
    let mut triangles: Vec<_> = vertices
        .chunks_exact(3)
        .map(|triangle| {
            let vertices = [triangle[0], triangle[1], triangle[2]];
            let average_depth = vertices
                .iter()
                .map(|vertex| vertex.position[2])
                .sum::<f32>()
                / 3.0;
            (average_depth, vertices)
        })
        .collect();
    triangles.sort_by(|a, b| a.0.total_cmp(&b.0));
    triangles
        .into_iter()
        .flat_map(|(_, triangle)| triangle)
        .collect()
}

fn segment_prism(
    out: &mut Vec<ToolModelVertex>,
    start: [f32; 3],
    end: [f32; 3],
    radius: f32,
    dark: [f32; 3],
    light: [f32; 3],
) {
    let direction = normalize(sub(end, start));
    let reference = if direction[2].abs() < 0.85 {
        [0.0, 0.0, 1.0]
    } else {
        [0.0, 1.0, 0.0]
    };
    let side = scale(normalize(cross(direction, reference)), radius);
    let other = scale(normalize(cross(direction, side)), radius);
    let start_ring = ring(start, side, other);
    let end_ring = ring(end, side, other);
    for face in 0..4 {
        let next = (face + 1) % 4;
        quad(
            out,
            [
                start_ring[face],
                start_ring[next],
                end_ring[next],
                end_ring[face],
            ],
            if face % 2 == 0 { light } else { dark },
        );
    }
    quad(out, start_ring, dark);
    quad(
        out,
        [end_ring[0], end_ring[3], end_ring[2], end_ring[1]],
        light,
    );
}

fn shovel_head(out: &mut Vec<ToolModelVertex>, center: [f32; 3]) {
    let outline = [
        [-0.060, 0.044],
        [0.060, 0.044],
        [0.073, 0.005],
        [0.043, -0.078],
        [0.0, -0.103],
        [-0.043, -0.078],
        [-0.073, 0.005],
    ];
    let front = outline.map(|point| [center[0] + point[0], center[1] + point[1], center[2]]);
    let back = front.map(|point| [point[0], point[1], point[2] - 0.016]);
    for index in 0..outline.len() {
        let next = (index + 1) % outline.len();
        quad(
            out,
            [front[index], front[next], back[next], back[index]],
            [0.22, 0.24, 0.25],
        );
        triangle(
            out,
            front[0],
            front[index],
            front[next],
            if index % 2 == 0 {
                [0.55, 0.58, 0.59]
            } else {
                [0.42, 0.45, 0.46]
            },
        );
    }
}

fn pickaxe_head(out: &mut Vec<ToolModelVertex>, center: [f32; 3]) {
    let left = [center[0] - 0.125, center[1] + 0.035, center[2]];
    let right = [center[0] + 0.125, center[1] + 0.035, center[2] - 0.008];
    segment_prism(
        out,
        left,
        right,
        0.022,
        [0.24, 0.27, 0.29],
        [0.59, 0.63, 0.65],
    );
    triangle(
        out,
        [left[0] - 0.035, left[1] - 0.012, left[2]],
        left,
        [left[0] + 0.055, left[1] - 0.014, left[2]],
        [0.44, 0.47, 0.49],
    );
    triangle(
        out,
        [right[0] - 0.055, right[1] - 0.014, right[2]],
        right,
        [right[0] + 0.035, right[1] - 0.012, right[2]],
        [0.49, 0.52, 0.54],
    );
}

fn ring(center: [f32; 3], side: [f32; 3], other: [f32; 3]) -> [[f32; 3]; 4] {
    [
        add(add(center, side), other),
        add(sub(center, side), other),
        sub(sub(center, side), other),
        add(sub(center, other), side),
    ]
}

fn quad(out: &mut Vec<ToolModelVertex>, corners: [[f32; 3]; 4], color: [f32; 3]) {
    triangle(out, corners[0], corners[1], corners[2], color);
    triangle(out, corners[0], corners[2], corners[3], color);
}

fn triangle(
    out: &mut Vec<ToolModelVertex>,
    a: [f32; 3],
    b: [f32; 3],
    c: [f32; 3],
    color: [f32; 3],
) {
    for position in [a, b, c] {
        out.push(ToolModelVertex { position, color });
    }
}

fn add(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|axis| a[axis] + b[axis])
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|axis| a[axis] - b[axis])
}

fn scale(value: [f32; 3], amount: f32) -> [f32; 3] {
    value.map(|component| component * amount)
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn normalize(value: [f32; 3]) -> [f32; 3] {
    let length = value.iter().map(|part| part * part).sum::<f32>().sqrt();
    if length <= f32::EPSILON {
        [0.0, 0.0, 1.0]
    } else {
        value.map(|part| part / length)
    }
}
