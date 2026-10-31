//! Cube-sphere cells: seams and poles use the same six-face traversal.
use glam::DVec3;

pub(crate) const BASE_LEVEL: u8 = 5;
pub(crate) const CLUSTER_LEVEL: u8 = 8;
pub(crate) const MAX_LEVEL: u8 = 12;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub(crate) struct Cell {
    pub(crate) face: u8,
    pub(crate) level: u8,
    pub(crate) x: u16,
    pub(crate) y: u16,
}

impl Cell {
    pub(crate) fn direction(self, x: f64, y: f64) -> DVec3 {
        let n = f64::from(1_u32 << self.level);
        let u = (f64::from(self.x) + x) / n * 2.0 - 1.0;
        let v = (f64::from(self.y) + y) / n * 2.0 - 1.0;
        match self.face {
            0 => DVec3::new(1.0, u, v),
            1 => DVec3::new(-1.0, u, v),
            2 => DVec3::new(u, 1.0, v),
            3 => DVec3::new(u, -1.0, v),
            4 => DVec3::new(u, v, 1.0),
            _ => DVec3::new(u, v, -1.0),
        }
        .normalize()
    }

    pub(crate) fn center(self) -> DVec3 {
        self.direction(0.5, 0.5)
    }

    /// Conservative angular radius; normalization of a cube face is 1-Lipschitz.
    pub(crate) fn radius(self) -> f64 {
        (2.0_f64.sqrt() / f64::from(1_u32 << self.level))
            .asin()
            .min(std::f64::consts::PI)
    }

    pub(crate) fn children(self) -> [Self; 4] {
        std::array::from_fn(|i| Self {
            face: self.face,
            level: self.level + 1,
            x: self.x * 2 + (i % 2) as u16,
            y: self.y * 2 + (i / 2) as u16,
        })
    }

    pub(crate) fn intersects(self, up: DVec3, radius: f64) -> bool {
        let reach = (radius + self.radius()).min(std::f64::consts::PI);
        self.center().dot(up) >= reach.cos()
    }
}

pub(crate) fn cap(up: DVec3, radius: f64, level: u8) -> Vec<Cell> {
    fn visit(cell: Cell, up: DVec3, radius: f64, level: u8, result: &mut Vec<Cell>) {
        if cell.level > 0 && !cell.intersects(up, radius) {
            return;
        }
        if cell.level == level {
            result.push(cell);
            return;
        }
        for child in cell.children() {
            visit(child, up, radius, level, result);
        }
    }
    let mut result = Vec::new();
    for face in 0..6 {
        visit(
            Cell {
                face,
                level: 0,
                x: 0,
                y: 0,
            },
            up,
            radius,
            level,
            &mut result,
        );
    }
    result
}
