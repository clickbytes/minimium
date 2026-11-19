//! Small bounded mesh builders; face shading is baked in linear RGB.
use super::ToolModelVertex;

pub(super) fn prism(
    out: &mut Vec<ToolModelVertex>,
    start: [f32; 3],
    end: [f32; 3],
    radii: [f32; 2],
    sides: usize,
    color: [f32; 3],
) {
    let direction = normalize(sub(end, start));
    let reference = if direction[2].abs() < 0.85 {
        [0.0, 0.0, 1.0]
    } else {
        [0.0, 1.0, 0.0]
    };
    let side = normalize(cross(direction, reference));
    let other = cross(direction, side);
    let ring = |center, radius| -> Vec<[f32; 3]> {
        (0..sides)
            .map(|index| {
                let angle = index as f32 * std::f32::consts::TAU / sides as f32;
                add(
                    center,
                    add(
                        scale(side, radius * angle.cos()),
                        scale(other, radius * angle.sin()),
                    ),
                )
            })
            .collect()
    };
    let a = ring(start, radii[0]);
    let b = ring(end, radii[1]);
    for index in 0..sides {
        let next = (index + 1) % sides;
        if radii[1] == 0.0 {
            triangle(out, a[index], a[next], end, color);
        } else {
            quad(out, [a[index], a[next], b[next], b[index]], color);
        }
    }
    for index in 1..sides - 1 {
        triangle(out, a[0], a[index + 1], a[index], color);
        if radii[1] > 0.0 {
            triangle(out, b[0], b[index], b[index + 1], color);
        }
    }
}

pub(super) fn quad(out: &mut Vec<ToolModelVertex>, corners: [[f32; 3]; 4], color: [f32; 3]) {
    triangle(out, corners[0], corners[1], corners[2], color);
    triangle(out, corners[0], corners[2], corners[3], color);
}

pub(super) fn triangle(
    out: &mut Vec<ToolModelVertex>,
    a: [f32; 3],
    b: [f32; 3],
    c: [f32; 3],
    color: [f32; 3],
) {
    let normal = normalize(cross(sub(b, a), sub(c, a)));
    let light = normalize([-0.35, 0.65, 0.68]);
    let diffuse = normal
        .iter()
        .zip(light)
        .map(|(a, b)| a * b)
        .sum::<f32>()
        .max(0.0);
    let color = color.map(|value| value * (0.55 + 0.45 * diffuse));
    out.extend([a, b, c].map(|position| ToolModelVertex { position, color }));
}

pub(super) fn add(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|axis| a[axis] + b[axis])
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|axis| a[axis] - b[axis])
}

pub(super) fn scale(value: [f32; 3], amount: f32) -> [f32; 3] {
    value.map(|part| part * amount)
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
    value.map(|part| part / length)
}
