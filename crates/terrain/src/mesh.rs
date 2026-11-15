//! Face-decided Marching Cubes. Shared faces use the same four scalar samples.
use crate::{TerrainEditError, TerrainView};
use minimium_world::terrain::{TerrainChunkKey, TerrainMaterial, TERRAIN_CELL_M};

const CORNERS: [[i32; 3]; 8] = [
    [0, 0, 0],
    [1, 0, 0],
    [1, 1, 0],
    [0, 1, 0],
    [0, 0, 1],
    [1, 0, 1],
    [1, 1, 1],
    [0, 1, 1],
];
const EDGES: [[usize; 2]; 12] = [
    [0, 1],
    [1, 2],
    [2, 3],
    [3, 0],
    [4, 5],
    [5, 6],
    [6, 7],
    [7, 4],
    [0, 4],
    [1, 5],
    [2, 6],
    [3, 7],
];
// Each face uses ascending world axes; opposite faces have identical ordering.
const FACES: [([usize; 4], [usize; 4]); 6] = [
    ([0, 1, 2, 3], [0, 1, 2, 3]),
    ([4, 5, 6, 7], [4, 5, 6, 7]),
    ([0, 1, 5, 4], [0, 9, 4, 8]),
    ([3, 2, 6, 7], [2, 10, 6, 11]),
    ([0, 3, 7, 4], [3, 11, 7, 8]),
    ([1, 2, 6, 5], [1, 10, 5, 9]),
];

#[derive(Clone, Copy, Debug)]
pub struct TerrainVertex {
    pub position_m: [f64; 3],
    pub normal: [f64; 3],
    pub material: TerrainMaterial,
}

#[derive(Clone, Debug, Default)]
pub struct TerrainMesh {
    pub triangles: Vec<[TerrainVertex; 3]>,
}

impl TerrainView<'_> {
    /// Reads a one-sample halo from canonical addresses, including unloaded base.
    pub fn mesh(&self, key: TerrainChunkKey) -> Result<TerrainMesh, TerrainEditError> {
        if key.world != self.store.world || !key.valid() {
            return Err(TerrainEditError::Invalid);
        }
        let mut mesh = TerrainMesh::default();
        for z in 0..32 {
            for y in 0..32 {
                for x in 0..32 {
                    let cell = std::array::from_fn(|i| key.coordinates[i] * 32 + [x, y, z][i]);
                    self.mesh_cell(cell, &mut mesh)?;
                }
            }
        }
        Ok(mesh)
    }

    fn mesh_cell(&self, cell: [i32; 3], mesh: &mut TerrainMesh) -> Result<(), TerrainEditError> {
        let mut values = [0.0; 8];
        let mut materials = [TerrainMaterial::Empty; 8];
        let points: [[f64; 3]; 8] = std::array::from_fn(|i| {
            std::array::from_fn(|axis| f64::from(cell[axis] + CORNERS[i][axis]) * TERRAIN_CELL_M)
        });
        for i in 0..8 {
            let sample = self
                .lattice_sample(std::array::from_fn(|axis| cell[axis] + CORNERS[i][axis]))
                .ok_or(TerrainEditError::Unavailable)?;
            // A globally identical tie rule avoids zero-length edges at exact zeros.
            values[i] = if sample.distance_m == 0.0 {
                1.0e-10
            } else {
                sample.distance_m
            };
            materials[i] = sample.material;
        }
        let crosses = EDGES.map(|[a, b]| (values[a] > 0.0) != (values[b] > 0.0));
        if !crosses.contains(&true) {
            return Ok(());
        }
        let mut vertices = [None; 12];
        for (edge, &[mut a, mut b]) in EDGES.iter().enumerate() {
            if !crosses[edge] {
                continue;
            }
            // Canonical interpolation order makes a shared edge bit-identical.
            if CORNERS[a] > CORNERS[b] {
                std::mem::swap(&mut a, &mut b);
            }
            let t = values[a] / (values[a] - values[b]);
            let position_m =
                std::array::from_fn(|i| points[a][i] + (points[b][i] - points[a][i]) * t);
            vertices[edge] = Some(TerrainVertex {
                position_m,
                normal: self.normal(position_m).unwrap_or([0.0, 1.0, 0.0]),
                material: materials[if values[a] < 0.0 { a } else { b }],
            });
        }
        let mut neighbors = [[usize::MAX; 2]; 12];
        for (corners, edges) in FACES {
            let active: Vec<_> = edges.into_iter().filter(|&e| crosses[e]).collect();
            match active.len() {
                2 => connect(&mut neighbors, active[0], active[1]),
                4 => {
                    let [a, b, c, d] = corners.map(|i| values[i]);
                    if a * c - b * d >= 0.0 {
                        connect(&mut neighbors, edges[0], edges[1]);
                        connect(&mut neighbors, edges[2], edges[3]);
                    } else {
                        connect(&mut neighbors, edges[0], edges[3]);
                        connect(&mut neighbors, edges[1], edges[2]);
                    }
                }
                _ => {}
            }
        }
        let mut visited = [false; 12];
        for first in 0..12 {
            if !crosses[first] || visited[first] {
                continue;
            }
            let mut ring = Vec::new();
            let mut previous = usize::MAX;
            let mut current = first;
            while !visited[current] {
                visited[current] = true;
                ring.push(vertices[current].ok_or(TerrainEditError::Invalid)?);
                let next = neighbors[current]
                    .into_iter()
                    .find(|&n| n != previous)
                    .filter(|&n| n < 12)
                    .ok_or(TerrainEditError::Invalid)?;
                previous = current;
                current = next;
            }
            if ring.len() < 3 || current != first {
                return Err(TerrainEditError::Invalid);
            }
            let position_m = std::array::from_fn(|i| {
                ring.iter().map(|v| v.position_m[i]).sum::<f64>() / ring.len() as f64
            });
            let center = TerrainVertex {
                position_m,
                normal: self.normal(position_m).unwrap_or(ring[0].normal),
                material: ring[0].material,
            };
            for index in 0..ring.len() {
                let mut triangle = [center, ring[index], ring[(index + 1) % ring.len()]];
                let a: [f64; 3] =
                    std::array::from_fn(|i| triangle[1].position_m[i] - position_m[i]);
                let b: [f64; 3] =
                    std::array::from_fn(|i| triangle[2].position_m[i] - position_m[i]);
                let normal = [
                    a[1] * b[2] - a[2] * b[1],
                    a[2] * b[0] - a[0] * b[2],
                    a[0] * b[1] - a[1] * b[0],
                ];
                if normal
                    .iter()
                    .zip(center.normal)
                    .map(|(a, b)| a * b)
                    .sum::<f64>()
                    < 0.0
                {
                    triangle.swap(1, 2);
                }
                mesh.triangles.push(triangle);
            }
        }
        Ok(())
    }
}

fn connect(neighbors: &mut [[usize; 2]; 12], a: usize, b: usize) {
    for (a, b) in [(a, b), (b, a)] {
        let index = usize::from(neighbors[a][0] != usize::MAX);
        neighbors[a][index] = b;
    }
}
