//! Sample-owned, closed camera-local tool meshes in metres.
use minimium_world::items::ToolKind;

mod mesh;
use mesh::{add, prism, scale, triangle};

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

/// Both tools share a hand pivot at [0.34, -0.32, -0.55] camera-local metres.
pub fn tool_model_catalog() -> [ToolModelDefinition; 2] {
    TOOL_MODEL_KINDS.map(|kind| ToolModelDefinition {
        kind,
        vertices: vertices(kind),
    })
}

pub fn tool_model_index(kind: ToolKind) -> Option<usize> {
    TOOL_MODEL_KINDS.iter().position(|entry| *entry == kind)
}

fn vertices(kind: ToolKind) -> Vec<ToolModelVertex> {
    let mut out = Vec::with_capacity(480);
    match kind {
        ToolKind::Shovel => shovel(&mut out),
        ToolKind::Pickaxe => pickaxe(&mut out),
    }
    out
}

const WOOD: [f32; 3] = [0.42, 0.22, 0.085];
const GRIP: [f32; 3] = [0.075, 0.055, 0.038];
const STEEL: [f32; 3] = [0.44, 0.49, 0.53];
const EDGE: [f32; 3] = [0.67, 0.72, 0.75];

fn shovel(out: &mut Vec<ToolModelVertex>) {
    // Tapered wooden shaft, deep metal socket, and a three-sided D grip.
    prism(
        out,
        [0.35, -0.18, -0.66],
        [0.155, 0.10, -0.94],
        [0.020, 0.016],
        6,
        WOOD,
    );
    prism(
        out,
        [0.18, 0.05, -0.89],
        [0.14, 0.14, -0.98],
        [0.026, 0.021],
        6,
        STEEL,
    );
    let root = [0.35, -0.18, -0.66];
    let butt = [0.40, -0.30, -0.56];
    let side = [0.060, 0.015, 0.0];
    let left = add(butt, scale(side, -1.0));
    let right = add(butt, side);
    prism(
        out,
        add(root, scale(side, -0.25)),
        left,
        [0.014, 0.016],
        4,
        GRIP,
    );
    prism(
        out,
        add(root, scale(side, 0.25)),
        right,
        [0.014, 0.016],
        4,
        GRIP,
    );
    prism(out, left, right, [0.020, 0.020], 6, WOOD);
    shovel_blade(out);
}

fn shovel_blade(out: &mut Vec<ToolModelVertex>) {
    // A cupped blade, not an unclosed planar fan: front, back and bevels.
    let outline = [
        [-0.055, -0.070],
        [0.055, -0.070],
        [0.105, 0.015],
        [0.105, 0.180],
        [0.065, 0.245],
        [0.0, 0.270],
        [-0.065, 0.245],
        [-0.105, 0.180],
        [-0.105, 0.015],
    ];
    let center = [0.13, 0.13, -0.99];
    let front = outline.map(|p| add(center, [p[0], p[1], 0.0]));
    let back = front.map(|p| add(p, [0.0, 0.0, -0.012]));
    let bowl_front = add(center, [0.0, 0.11, -0.030]);
    let bowl_back = add(bowl_front, [0.0, 0.0, -0.012]);
    for index in 0..outline.len() {
        let next = (index + 1) % outline.len();
        triangle(out, bowl_front, front[index], front[next], STEEL);
        triangle(out, bowl_back, back[next], back[index], [0.27, 0.31, 0.34]);
        mesh::quad(
            out,
            [front[index], back[index], back[next], front[next]],
            EDGE,
        );
    }
    // Stepping shoulders at the neck strengthen the blade silhouette.
    for sign in [-1.0, 1.0] {
        prism(
            out,
            add(center, [sign * 0.035, -0.060, 0.006]),
            add(center, [sign * 0.090, -0.035, 0.006]),
            [0.012, 0.012],
            4,
            STEEL,
        );
    }
}

fn pickaxe(out: &mut Vec<ToolModelVertex>) {
    let foot = [0.41, -0.44, -0.43];
    let head = [0.14, 0.16, -0.96];
    prism(out, foot, head, [0.025, 0.019], 8, WOOD);
    prism(
        out,
        [0.395, -0.405, -0.465],
        [0.335, -0.275, -0.585],
        [0.028, 0.025],
        6,
        GRIP,
    );
    prism(
        out,
        [0.151, 0.125, -0.925],
        [0.123, 0.203, -1.003],
        [0.042, 0.034],
        6,
        STEEL,
    );
    // Forged arms curve away from the eye and taper to solid pointed tips.
    for sign in [-1.0, 1.0] {
        let shoulder = add(head, [sign * 0.090, 0.014, -0.005]);
        let elbow = add(head, [sign * 0.185, -0.018, -0.026]);
        let tip = add(head, [sign * 0.275, -0.090, -0.060]);
        prism(
            out,
            add(head, [sign * 0.015, 0.0, 0.0]),
            shoulder,
            [0.032, 0.028],
            4,
            STEEL,
        );
        prism(out, shoulder, elbow, [0.028, 0.018], 4, STEEL);
        prism(out, elbow, tip, [0.018, 0.0], 4, EDGE);
    }
}
