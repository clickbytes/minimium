use super::Request;
use crate::environment::weather::wind::EARTH_RADIUS;
use glam::DVec3;
use std::{cell::RefCell, collections::HashSet};

pub(super) const MAX_TILES: usize = 512;
pub(super) const GRID: usize = 6;

#[derive(Clone, Copy, Eq, PartialEq, Hash)]
pub(super) struct Tile {
    pub(super) face: u8,
    pub(super) level: u8,
    pub(super) x: u32,
    pub(super) y: u32,
}

impl Tile {
    pub(super) fn uv(self, x: f64, y: f64) -> [f64; 2] {
        let width = 2.0 / f64::from(1u32 << self.level);
        [
            -1.0 + (f64::from(self.x) + x) * width,
            -1.0 + (f64::from(self.y) + y) * width,
        ]
    }

    pub(super) fn center(self) -> DVec3 {
        direction(self.face, self.uv(0.5, 0.5))
    }

    fn angular_size(self) -> f64 {
        2.0 / f64::from(1u32 << self.level)
    }

    fn visible(self, request: &Request) -> bool {
        let eye = request.eye;
        // Conservative horizon culling includes the full 18 km atmosphere.
        let horizon = (request.radius / eye.length().max(request.radius)).acos();
        let cloud_horizon = (EARTH_RADIUS / (EARTH_RADIUS + 18000.0)).acos();
        let angular_distance = self
            .center()
            .dot(eye.normalize_or_zero())
            .clamp(-1.0, 1.0)
            .acos();
        angular_distance < horizon + cloud_horizon + self.angular_size() * 1.5
    }

    fn priority(self, request: &Request) -> f64 {
        if self.level >= 13 || !self.visible(request) {
            return 0.0;
        }
        let size = self.angular_size() * request.radius;
        let center = self.center() * (request.radius * (1.0 + 4000.0 / EARTH_RADIUS));
        let distance = ((center - request.eye).length() - size * 0.8)
            .max(request.radius * 200.0 / EARTH_RADIUS);
        size / distance * request.pixels_per_radian
    }

    fn children(self) -> [Self; 4] {
        std::array::from_fn(|i| Self {
            face: self.face,
            level: self.level + 1,
            x: self.x * 2 + i as u32 % 2,
            y: self.y * 2 + i as u32 / 2,
        })
    }
}

pub(super) fn select(request: &Request) -> Vec<Tile> {
    // Retain splits within a 20% screen-size band. The topology must not chatter
    // when a slow orbit/zoom crosses a single threshold in successive jobs.
    thread_local! {
        static PREVIOUS: RefCell<Option<(Request, HashSet<Tile>)>> = const { RefCell::new(None) };
    }
    PREVIOUS.with_borrow_mut(|previous| {
        let retained = previous
            .as_ref()
            .filter(|(old, _)| old.same_scene(request) && old.standard == request.standard)
            .map(|(_, splits)| splits);
        let tiles = select_with_history(request, retained);
        let mut splits = HashSet::new();
        for &tile in &tiles {
            let mut parent = tile;
            while parent.level > 0 {
                parent = Tile {
                    level: parent.level - 1,
                    x: parent.x / 2,
                    y: parent.y / 2,
                    ..parent
                };
                splits.insert(parent);
            }
        }
        *previous = Some((request.clone(), splits));
        tiles
    })
}

fn select_with_history(request: &Request, retained: Option<&HashSet<Tile>>) -> Vec<Tile> {
    let mut tiles: Vec<_> = (0..6)
        .map(|face| Tile {
            face,
            level: 0,
            x: 0,
            y: 0,
        })
        .collect();
    let budget = request.budget.min(MAX_TILES);
    let threshold = if request.standard { 72.0 } else { 120.0 };
    while tiles.len() + 3 <= budget {
        let next = tiles
            .iter()
            .enumerate()
            .map(|(i, tile)| {
                let band = if retained.is_some_and(|splits| splits.contains(tile)) {
                    0.8
                } else {
                    1.0
                };
                (i, tile.priority(request) / band)
            })
            .max_by(|a, b| a.1.total_cmp(&b.1));
        let Some((index, priority)) = next else { break };
        if priority <= threshold {
            break;
        }
        let parent = tiles.swap_remove(index);
        // Retain a complete coarse globe while a different view is prepared.
        tiles.extend(parent.children());
    }
    tiles
}

/// Shared cube-face boundaries avoid longitude seams and pole degeneracy.
pub(super) fn direction(face: u8, [u, v]: [f64; 2]) -> DVec3 {
    match face {
        0 => DVec3::new(1.0, v, u),
        1 => DVec3::new(-1.0, v, u),
        2 => DVec3::new(u, 1.0, v),
        3 => DVec3::new(u, -1.0, v),
        4 => DVec3::new(u, v, 1.0),
        _ => DVec3::new(u, v, -1.0),
    }
    .normalize()
}
