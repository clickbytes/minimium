//! Bounded temporal mesh cache; unchanged cloud windows need no density work.
use super::{Face, MAX_FACES};
use minimium_environment::weather::{CloudSnapshot, DensityField};
use minimium_renderer::cloud_voxels::Voxels;
use std::collections::{HashMap, HashSet};

const MAX_SAMPLES: usize = 8_000_000;
const MAX_CACHE_FACES: usize = MAX_FACES;

#[derive(Clone, Copy, Default)]
pub(super) struct Morph {
    pub start: f64,
    pub step: f64,
    pub level: u8,
}

#[derive(Default)]
pub(super) struct MeshStats {
    pub samples: usize,
    pub bricks: usize,
    pub cached: usize,
    pub simplified: usize,
    pub levels: [usize; 7],
    pub cache_bytes: usize,
}

struct Entry {
    window: i64,
    level: u8,
    budget: usize,
    faces: Vec<Face>,
    bricks: usize,
    simplified: bool,
    render_level: u8,
}

#[derive(Default)]
pub(super) struct MeshCache {
    entries: HashMap<u64, Entry>,
}

pub(super) struct MeshOutput {
    pub faces: Vec<Face>,
    pub morphs: Vec<Morph>,
    pub stats: MeshStats,
}

impl MeshCache {
    pub fn build(
        &mut self,
        snapshot: &CloudSnapshot,
        detail: u8,
        cancelled: impl Fn() -> bool,
    ) -> Option<MeshOutput> {
        let live: HashSet<_> = snapshot.clouds.iter().map(|c| c.id).collect();
        self.entries.retain(|id, _| live.contains(id));
        let mut cached_faces: usize = self.entries.values().map(|e| e.faces.capacity()).sum();
        let mut result = MeshOutput {
            faces: Vec::with_capacity(MAX_FACES),
            morphs: Vec::with_capacity(snapshot.clouds.len()),
            stats: MeshStats::default(),
        };
        let priorities: Vec<_> = snapshot
            .clouds
            .iter()
            .map(|c| {
                let distance = c
                    .direction_at(snapshot.utc)
                    .dot(snapshot.observer)
                    .clamp(-1.0, 1.0)
                    .acos();
                if distance < 0.006 {
                    (6, 16)
                } else if distance < 0.018 {
                    (5, 4)
                } else {
                    (4, 1)
                }
            })
            .collect();
        let weight: usize = priorities.iter().map(|(_, w)| w).sum();
        let available = MAX_FACES - snapshot.clouds.len() * 6;
        for (index, cloud) in snapshot.clouds.iter().copied().enumerate() {
            if cancelled() {
                return None;
            }
            let (wanted, priority) = priorities[index];
            let share = 6 + available * priority / weight.max(1);
            // Stable buckets prevent a birth/death changing every mesh budget by
            // one face and invalidating the entire otherwise reusable cache.
            let budget = (1_usize << share.ilog2()).max(6);
            let mut level = wanted.min(detail).clamp(cloud.min_level, cloud.max_level);
            let allowance = MAX_SAMPLES / snapshot.clouds.len().max(1);
            let cost = |n| {
                cloud.grid(n).0.iter().product::<usize>() * if n == cloud.min_level { 8 } else { 2 }
            };
            while level > cloud.min_level && cost(level) > allowance {
                level -= 1;
            }
            let (window, start, step) = cloud.morph_window(snapshot.utc);
            let reuse = self
                .entries
                .get(&cloud.id)
                .is_some_and(|e| e.window == window && e.level == level && e.budget == budget);
            if reuse {
                result.stats.cached += 1;
            } else {
                if let Some(old) = self.entries.remove(&cloud.id) {
                    cached_faces -= old.faces.capacity();
                }
                let mut bricks = 0;
                let mut render_level = level;
                let (faces, simplified) = if cost(level) <= allowance {
                    let mut cells = Voxels::build(cloud, level, start, step);
                    result.stats.samples += cost(level);
                    bricks = cells.bricks;
                    let mut faces = minimium_renderer::cloud_mesh::build(cloud, &cells, budget);
                    let simplified = faces.is_none();
                    for stride in [2, 4, 8, 16, 32, 64] {
                        if faces.is_some() {
                            break;
                        }
                        cells.simplify(stride);
                        render_level = level.saturating_sub(stride.ilog2() as u8);
                        faces = minimium_renderer::cloud_mesh::build(cloud, &cells, budget);
                    }
                    if faces.as_ref().is_some_and(Vec::is_empty)
                        && cells.mean.iter().any(|v| *v > 0.0)
                    {
                        faces = None;
                        render_level = 0;
                    }
                    (
                        faces.unwrap_or_else(|| {
                            minimium_renderer::cloud_mesh::envelope(cloud, cells.mean)
                        }),
                        simplified,
                    )
                } else {
                    render_level = 0;
                    // Fixed stratified integral when even the minimum grid exceeds work budget.
                    let fields = [
                        DensityField::new(cloud, start),
                        DensityField::new(cloud, start + step),
                    ];
                    let mut mean = [0.0; 2];
                    for z in 0..4 {
                        for y in 0..4 {
                            for x in 0..4 {
                                let p = glam::DVec3::new(
                                    (x as f64 + 0.5) / 4.0 - 0.5,
                                    (y as f64 + 0.5) / 4.0,
                                    (z as f64 + 0.5) / 4.0 - 0.5,
                                ) * cloud.size_m;
                                for t in 0..2 {
                                    mean[t] += fields[t].sample(p) as f32 / 64.0;
                                }
                            }
                        }
                    }
                    result.stats.samples += 128;
                    (minimium_renderer::cloud_mesh::envelope(cloud, mean), true)
                };
                let faces = faces.into_boxed_slice().into_vec();
                self.entries.insert(
                    cloud.id,
                    Entry {
                        window,
                        level,
                        budget,
                        faces,
                        bricks,
                        simplified,
                        render_level,
                    },
                );
            }
            let entry = &self.entries[&cloud.id];
            result.stats.bricks += entry.bricks;
            result.stats.simplified += usize::from(entry.simplified);
            result.stats.levels[entry.render_level as usize] += 1;
            result.morphs.push(Morph {
                start,
                step,
                level: entry.render_level,
            });
            for mut face in entry.faces.iter().copied() {
                face.center[3] = index as f32;
                result.faces.push(face);
            }
            if !reuse {
                let capacity = entry.faces.capacity();
                if cached_faces + capacity <= MAX_CACHE_FACES {
                    cached_faces += capacity;
                } else {
                    self.entries.remove(&cloud.id);
                }
            }
        }
        result.stats.cache_bytes = cached_faces * std::mem::size_of::<Face>()
            + self.entries.capacity() * (std::mem::size_of::<Entry>() + 16);
        Some(result)
    }
}
