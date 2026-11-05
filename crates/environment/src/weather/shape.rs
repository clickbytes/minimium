//! Continuous, bounded morphology. The same field is sampled by every voxel LOD.
use super::{climate::noise, hash, random, Cloud, CloudKind};
use glam::DVec3;

#[derive(Clone, Copy, Default)]
struct Lobe {
    center: DVec3,
    radius: DVec3,
}

pub struct DensityField {
    cloud: Cloud,
    lobes: [Lobe; 31],
    count: usize,
    phase: DVec3,
    drift: DVec3,
}

impl DensityField {
    pub fn new(cloud: Cloud, age: f64) -> Self {
        let phase = DVec3::new(
            random(cloud.id),
            random(cloud.id ^ 61),
            random(cloud.id ^ 93),
        ) * std::f64::consts::TAU;
        let evolution = match cloud.kind {
            CloudKind::Congestus | CloudKind::Cumulonimbus => 260.0,
            CloudKind::Cumulus => 360.0,
            CloudKind::Cirrus | CloudKind::Cirrocumulus => 500.0,
            CloudKind::Stratocumulus | CloudKind::Altocumulus => 600.0,
            _ => 900.0,
        };
        let time = age / evolution;
        let mut field = Self {
            cloud,
            lobes: [Lobe::default(); 31],
            count: 0,
            phase: phase + DVec3::new(time, time * 0.73, time * 0.47),
            drift: DVec3::new(
                (random(cloud.id ^ 0x3ab1) - 0.5) * 0.22,
                (random(cloud.id ^ 0x3ab2) - 0.5) * 0.10,
                (random(cloud.id ^ 0x3ab3) - 0.5) * 0.22,
            ) * time,
        };
        if matches!(
            cloud.kind,
            CloudKind::Cumulus | CloudKind::Congestus | CloudKind::Cumulonimbus
        ) {
            field.branch(
                DVec3::new(0.0, 0.33, 0.0),
                DVec3::new(0.56, 0.29, 0.56),
                0,
                cloud.id,
            );
        }
        field
    }

    fn branch(&mut self, center: DVec3, radius: DVec3, depth: u8, seed: u64) {
        let phase = random(seed) * std::f64::consts::TAU;
        let movement = DVec3::new(
            (self.phase.x + phase).sin(),
            (self.phase.y + phase).sin() * 0.4,
            (self.phase.z - phase).cos(),
        ) * radius
            * 0.16;
        let center = center + movement;
        self.lobes[self.count] = Lobe { center, radius };
        self.count += 1;
        if depth == 4 {
            return;
        }
        for child in 0..2 {
            let seed = hash(seed ^ ((child + 1) * 0x9e3779b9));
            let angle = random(seed ^ 7) * std::f64::consts::TAU;
            let offset = DVec3::new(
                angle.cos() * 0.75,
                0.6 + random(seed ^ 11) * 0.4,
                angle.sin() * 0.75,
            );
            self.branch(
                center + offset * radius,
                radius * (0.52 + random(seed ^ 19) * 0.15),
                depth + 1,
                seed,
            );
        }
    }

    /// Local metres, y measured above the cloud base. No state or frame delta.
    pub fn sample(&self, local: DVec3) -> f64 {
        let p = local / self.cloud.size_m * DVec3::new(2.0, 1.0, 2.0);
        if p.x.abs() >= 1.0 || p.z.abs() >= 1.0 || p.y <= 0.0 || p.y >= 1.0 {
            return 0.0;
        }
        // Bounded coherent domain deformation, not a fluid integration. Features
        // share the large-scale motion; independent phases prevent synchronized breathing.
        let q = p + DVec3::new(
            (p.z * 3.0 + p.y * 2.0 + self.phase.x).sin() * 0.07,
            (p.x * 2.4 + self.phase.y).sin() * 0.035,
            (p.x * 2.8 - p.y + self.phase.z).cos() * 0.06,
        );
        let coarse = noise(q * DVec3::new(3.5, 5.0, 3.5) + self.drift, self.cloud.id);
        let fine = noise(q * 11.0 + self.drift * 1.7, self.cloud.id ^ 0xaa37);
        let envelope = (1.0 - p.x.powi(4) - p.z.powi(4))
            .min(p.y * 12.0)
            .min((1.0 - p.y) * 12.0);
        use CloudKind::*;
        let shape = match self.cloud.kind {
            Cumulus | Congestus | Cumulonimbus => self.cumulus(q),
            Stratocumulus => self.cells(q, 2.6, 0.38) + 0.17,
            Altocumulus => self.cells(q, 3.7, 0.35),
            Cirrocumulus => self.cells(q, 5.5, 0.29),
            Lenticularis => self.lens(q),
            Cirrus => self.filaments(q),
            Stratus | Altostratus | Nimbostratus | Cirrostratus => {
                let half = if self.cloud.kind == Nimbostratus {
                    0.37
                } else {
                    0.24
                };
                let height = q.y - 0.5 - (q.x * 4.0 + self.phase.x * 0.3).sin() * 0.055;
                half - height.abs() - (0.38 - coarse).max(0.0) * 0.4
            }
        };
        let erosion = (coarse - 0.5) * 0.13 + (fine - 0.5) * 0.045;
        smooth(-0.035, 0.09, (shape + erosion).min(envelope - 0.05))
    }

    fn cumulus(&self, q: DVec3) -> f64 {
        let mut field: f64 = -1.0;
        for lobe in &self.lobes[..self.count] {
            field = field.max((1.0 - ((q - lobe.center) / lobe.radius).length_squared()) * 0.2);
        }
        if self.cloud.kind == CloudKind::Cumulonimbus {
            let anvil = DVec3::new((q.x - 0.12) / 0.83, (q.y - 0.82) / 0.15, q.z / 0.7);
            field = field.max((1.0 - anvil.length_squared()) * 0.16);
        }
        field.min((q.y - 0.09) * 2.0)
    }

    fn cells(&self, q: DVec3, frequency: f64, height: f64) -> f64 {
        let frequency = frequency * (0.85 + random(self.cloud.id ^ 0xc18a) * 0.3);
        let x = q.x * frequency;
        let z = q.z * frequency;
        let ix = x.round() as i64;
        let iz = z.round() as i64;
        // Neighbour kernels overlap continuously; nearest-cell-only noise would
        // jump whenever the deformation crosses a cell address boundary.
        let mut value: f64 = -1.0;
        for j in iz - 1..=iz + 1 {
            for i in ix - 1..=ix + 1 {
                let seed = self.cloud.id
                    ^ (i as u64).wrapping_mul(73856093)
                    ^ (j as u64).wrapping_mul(19349663);
                let dx = x - i as f64 - (random(seed) - 0.5) * 0.25;
                let dz = z - j as f64 - (random(seed ^ 3) - 0.5) * 0.25;
                let dy = (q.y - 0.45 - random(seed ^ 7) * 0.1) / height;
                value = value.max(
                    (0.65 + random(seed ^ 5) * 0.2 - dx * dx * 2.0 - dz * dz * 2.0 - dy * dy)
                        * 0.18,
                );
            }
        }
        value
    }

    fn lens(&self, q: DVec3) -> f64 {
        let layers = 2.0 + (random(self.cloud.id ^ 0xc18b) * 3.0).floor();
        let layer = (q.y * layers).fract() - 0.5;
        let radius = 1.0 - (q.y - 0.45).abs() * 0.8;
        (radius - q.x * q.x - q.z * q.z - layer.powi(2) * 8.0) * 0.18
    }

    fn filaments(&self, q: DVec3) -> f64 {
        let mut field: f64 = -1.0;
        let count = 3 + (random(self.cloud.id ^ 0xc18c) * 3.0) as u64;
        for i in 0..count {
            let phase = random(self.cloud.id ^ (i + 13)) * 4.0;
            let center = (i as f64 - (count - 1) as f64 * 0.5) * (1.4 / count as f64)
                + (q.x * 2.5 + self.phase.z * 0.35 + phase).sin() * 0.15;
            let y = 0.45 + (q.x * 3.0 + phase + self.phase.y * 0.2).sin() * 0.13;
            let taper = (1.0 - q.x.abs()).max(0.0);
            let width = 0.065 + taper * 0.09;
            let distance = ((q.z - center) / width).powi(2) + ((q.y - y) / 0.19).powi(2);
            field = field.max((1.0 - distance) * 0.15);
        }
        field
    }
}

fn smooth(low: f64, high: f64, value: f64) -> f64 {
    let t = ((value - low) / (high - low)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}
