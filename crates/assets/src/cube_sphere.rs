//! Bounded cube-sphere tessellation. Generated triangles are visual-only;
//! radial surface contact is solved analytically in the shared character controller.
use minimium_core::world::{CubeSphere, MapTriangle, MAX_MAP_TRIANGLES};

pub(super) const MIN_SUBDIVISIONS: usize = 4;
pub(super) const MAX_SUBDIVISIONS: usize = 24;

pub(super) fn append_cube_sphere(
    triangles: &mut Vec<MapTriangle>,
    planet: CubeSphere,
    divisions: usize,
    color: [f32; 3],
) -> Result<(), String> {
    if !(MIN_SUBDIVISIONS..=MAX_SUBDIVISIONS).contains(&divisions)
        || triangles.len() + 12 * divisions * divisions > MAX_MAP_TRIANGLES
    {
        return Err("planet tessellation exceeds supported triangle limit".into());
    }
    for face in 0..6 {
        for row in 0..divisions {
            for col in 0..divisions {
                let x0 = 2.0 * col as f32 / divisions as f32 - 1.0;
                let x1 = 2.0 * (col + 1) as f32 / divisions as f32 - 1.0;
                let y0 = 2.0 * row as f32 / divisions as f32 - 1.0;
                let y1 = 2.0 * (row + 1) as f32 / divisions as f32 - 1.0;
                let a = point(planet, face, x0, y0);
                let b = point(planet, face, x1, y0);
                let c = point(planet, face, x0, y1);
                let d = point(planet, face, x1, y1);
                let shade = 0.83 + ((row + col + face) % 4) as f32 * 0.055;
                let tinted = color.map(|channel| (channel * shade).min(1.0));
                append_outward(triangles, [a, b, d], planet.center, tinted);
                append_outward(triangles, [a, d, c], planet.center, tinted);
            }
        }
    }
    Ok(())
}

fn point(planet: CubeSphere, face: usize, u: f32, v: f32) -> [f32; 3] {
    let cube = match face {
        0 => [1.0, u, v],
        1 => [-1.0, u, v],
        2 => [u, 1.0, v],
        3 => [u, -1.0, v],
        4 => [u, v, 1.0],
        _ => [u, v, -1.0],
    };
    let inverse = cube.iter().map(|x| x * x).sum::<f32>().sqrt().recip();
    [
        planet.center[0] + cube[0] * inverse * planet.radius,
        planet.center[1] + cube[1] * inverse * planet.radius,
        planet.center[2] + cube[2] * inverse * planet.radius,
    ]
}

fn append_outward(
    output: &mut Vec<MapTriangle>,
    mut vertices: [[f32; 3]; 3],
    center: [f32; 3],
    color: [f32; 3],
) {
    let [a, b, c] = vertices;
    let ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let ac = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
    let normal = [
        ab[1] * ac[2] - ab[2] * ac[1],
        ab[2] * ac[0] - ab[0] * ac[2],
        ab[0] * ac[1] - ab[1] * ac[0],
    ];
    let radial = [a[0] - center[0], a[1] - center[1], a[2] - center[2]];
    if normal.iter().zip(radial).map(|(a, b)| a * b).sum::<f32>() < 0.0 {
        vertices.swap(1, 2);
    }
    output.push(MapTriangle {
        vertices,
        color,
        solid: false,
    });
}
