use crate::{Reservoir, ReservoirKind, VolumeLevel};
use minimium_world::{fluid::WaterBodyId, world::WorldInstanceId};
use std::collections::BTreeMap;

mod transfer;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FluidError {
    Invalid,
    Capacity,
    Unavailable,
    InsufficientVolume,
    DisplacementRequired,
    RevisionExhausted,
}

#[derive(Clone, Copy, Debug)]
pub struct FluidConfig {
    pub max_active_cells: usize,
}

impl FluidConfig {
    pub const WORLD_DEFAULT: Self = Self {
        max_active_cells: 131_072,
    };
    pub const SERVER_DEFAULT: Self = Self {
        max_active_cells: 524_288,
    };
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CellWater {
    pub capacity_m3: f64,
    pub volume_m3: f64,
    /// Three positive-face velocities of the MAC lattice, in metres/second.
    pub positive_velocity_m_s: [f64; 3],
    /// Negative boundary-face velocities, used only when no active cell owns that face.
    pub negative_boundary_velocity_m_s: [f64; 3],
}

impl CellWater {
    pub fn valid(self) -> bool {
        (0.0..=0.125).contains(&self.capacity_m3)
            && (0.0..=self.capacity_m3).contains(&self.volume_m3)
            && self.positive_velocity_m_s.iter().all(|v| v.is_finite())
            && self
                .negative_boundary_velocity_m_s
                .iter()
                .all(|v| v.is_finite())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum WaterEndpoint {
    Cell([i32; 3]),
    Reservoir(WaterBodyId),
    Solid,
    /// Known open air; pressure is atmospheric, but it never stores/transfers water.
    Air,
}

/// One coherent checkpoint, containing local cells AND reservoir balances.
#[derive(Clone, Debug)]
pub struct FluidSnapshot {
    world: WorldInstanceId,
    revision: u64,
    config: FluidConfig,
    cells: BTreeMap<[i32; 3], CellWater>,
    reservoirs: BTreeMap<WaterBodyId, Reservoir>,
}

impl FluidSnapshot {
    pub fn new(world: WorldInstanceId, config: FluidConfig) -> Result<Self, FluidError> {
        if config.max_active_cells == 0 || config.max_active_cells > 524_288 {
            return Err(FluidError::Capacity);
        }
        Ok(Self {
            world,
            revision: 0,
            config,
            cells: BTreeMap::new(),
            reservoirs: BTreeMap::new(),
        })
    }

    /// Rebuilds a coherent checkpoint after persistence decoded cells and reservoirs.
    pub fn from_parts(
        world: WorldInstanceId,
        config: FluidConfig,
        revision: u64,
        cells: BTreeMap<[i32; 3], CellWater>,
        reservoirs: BTreeMap<WaterBodyId, Reservoir>,
    ) -> Result<Self, FluidError> {
        if config.max_active_cells == 0
            || config.max_active_cells > 524_288
            || cells.len() > config.max_active_cells
            || cells.values().any(|cell| !cell.valid())
            || reservoirs.len() > 65_536
            || reservoirs.iter().any(|(id, r)| *id != r.id || !r.valid())
            || cells
                .values()
                .map(|c| c.volume_m3)
                .sum::<f64>()
                .is_infinite()
        {
            return Err(FluidError::Invalid);
        }
        Ok(Self {
            world,
            revision,
            config,
            cells,
            reservoirs,
        })
    }

    pub fn config(&self) -> FluidConfig {
        self.config
    }
    pub fn replace_velocity(
        &mut self,
        key: [i32; 3],
        velocity: [f64; 3],
    ) -> Result<(), FluidError> {
        let negative = self
            .cells
            .get(&key)
            .ok_or(FluidError::Unavailable)?
            .negative_boundary_velocity_m_s;
        self.replace_face_velocities(key, velocity, negative)
    }

    pub fn replace_face_velocities(
        &mut self,
        key: [i32; 3],
        positive: [f64; 3],
        negative: [f64; 3],
    ) -> Result<(), FluidError> {
        if positive
            .iter()
            .chain(negative.iter())
            .any(|v| !v.is_finite())
        {
            return Err(FluidError::Invalid);
        }
        let current = self.cells.get(&key).ok_or(FluidError::Unavailable)?;
        if current.positive_velocity_m_s == positive
            && current.negative_boundary_velocity_m_s == negative
        {
            return Ok(());
        }
        let next = self.next_revision()?;
        let cell = self.cells.get_mut(&key).ok_or(FluidError::Unavailable)?;
        cell.positive_velocity_m_s = positive;
        cell.negative_boundary_velocity_m_s = negative;
        self.revision = next;
        Ok(())
    }

    pub fn world(&self) -> WorldInstanceId {
        self.world
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn cells(&self) -> &BTreeMap<[i32; 3], CellWater> {
        &self.cells
    }

    pub fn reservoirs(&self) -> &BTreeMap<WaterBodyId, Reservoir> {
        &self.reservoirs
    }

    pub fn register_reservoir(&mut self, reservoir: Reservoir) -> Result<(), FluidError> {
        if !reservoir.valid() || self.reservoirs.contains_key(&reservoir.id) {
            return Err(FluidError::Invalid);
        }
        if self.reservoirs.len() >= 65_536 {
            return Err(FluidError::Capacity);
        }
        let next = self.next_revision()?;
        self.reservoirs.insert(reservoir.id, reservoir);
        self.revision = next;
        Ok(())
    }

    /// Removes active-cell water from a finite reservoir's remaining level curve.
    /// The current level is preserved when the same amount is subsequently
    /// transferred into the cell; ocean reservoirs have a fixed level and need
    /// no curve adjustment.
    /// Transfers water into an active cell while excluding that cell's
    /// level-dependent volume from a finite reservoir curve atomically.
    pub fn transfer_from_reservoir_with_curve(
        &mut self,
        id: WaterBodyId,
        cell: [i32; 3],
        amount_m3: f64,
        active_volume_at_level: impl FnMut(f64) -> f64,
    ) -> Result<(), FluidError> {
        let mut staged = self.clone();
        staged.carve_finite_reservoir_curve(id, amount_m3, active_volume_at_level)?;
        staged.transfer(
            WaterEndpoint::Reservoir(id),
            WaterEndpoint::Cell(cell),
            amount_m3,
        )?;
        *self = staged;
        Ok(())
    }

    fn carve_finite_reservoir_curve(
        &mut self,
        id: WaterBodyId,
        amount_m3: f64,
        mut active_volume_at_level: impl FnMut(f64) -> f64,
    ) -> Result<(), FluidError> {
        if !amount_m3.is_finite() || amount_m3 <= 0.0 {
            return Err(FluidError::Invalid);
        }
        let current_level = self
            .reservoirs
            .get(&id)
            .ok_or(FluidError::Unavailable)?
            .level_m()?;
        let revision = self.next_revision()?;
        let reservoir = self
            .reservoirs
            .get_mut(&id)
            .ok_or(FluidError::Unavailable)?;
        let ReservoirKind::Finite { curve, volume_m3 } = &mut reservoir.kind else {
            return Ok(());
        };
        let current_high = *volume_m3;
        let current_low = reservoir.volume_correction_m3;
        let current = current_high + current_low;
        let base_level = curve[0].level_m;
        let current_point = VolumeLevel {
            level_m: current_level,
            volume_m3: current,
        };
        let removed_current = active_volume_at_level(current_level);
        if !removed_current.is_finite() || removed_current < 0.0 {
            return Err(FluidError::Invalid);
        }
        let mut points = curve.clone();
        if current_point.level_m > base_level
            && !points
                .iter()
                .any(|point| point.level_m == current_point.level_m)
        {
            points.push(current_point);
            points.sort_by(|a, b| a.level_m.total_cmp(&b.level_m));
        }
        while points.len() > 4096 {
            let remove = (1..points.len() - 1)
                .filter(|index| points[*index].level_m != current_level)
                .min_by(|a, b| {
                    let span_a = points[a + 1].volume_m3 - points[a - 1].volume_m3;
                    let span_b = points[b + 1].volume_m3 - points[b - 1].volume_m3;
                    span_a.total_cmp(&span_b)
                })
                .ok_or(FluidError::Capacity)?;
            points.remove(remove);
        }
        let mut adjusted = Vec::with_capacity(points.len());
        adjusted.push(VolumeLevel {
            level_m: base_level,
            volume_m3: 0.0,
        });
        for point in points.iter().skip(1) {
            let removed = if point.level_m == current_level {
                removed_current
            } else {
                active_volume_at_level(point.level_m)
            };
            if !removed.is_finite() || removed < 0.0 {
                return Err(FluidError::Invalid);
            }
            let (source_high, source_low) = if point.level_m == current_level {
                (current_high, current_low)
            } else {
                (point.volume_m3, 0.0)
            };
            let volume = Reservoir::debit_ceiling(source_high, source_low, removed)
                .ok_or(FluidError::Invalid)?;
            if !volume.is_finite() || volume < adjusted.last().unwrap().volume_m3 {
                return Err(FluidError::Capacity);
            }
            adjusted.push(VolumeLevel {
                level_m: point.level_m,
                volume_m3: volume,
            });
        }
        let current_after_carve =
            Reservoir::debit_ceiling(current_high, current_low, removed_current)
                .ok_or(FluidError::Invalid)?;
        if current_after_carve > 0.0 {
            adjusted.retain(|point| {
                point.volume_m3 != current_after_carve || point.level_m == current_level
            });
        }
        if adjusted.len() < 2
            || adjusted.len() > 4096
            || (removed_current - amount_m3).abs()
                > 1.0e-12 * amount_m3.abs().max(removed_current.abs()).max(1.0e-12)
            || !current_after_carve.is_finite()
            || current_after_carve < 0.0
            || current_after_carve > adjusted.last().unwrap().volume_m3
        {
            return Err(FluidError::Capacity);
        }
        *curve = adjusted;
        self.revision = revision;
        Ok(())
    }

    /// New space is dry. Filling must transfer existing water from an endpoint.
    pub fn activate(&mut self, key: [i32; 3], capacity_m3: f64) -> Result<(), FluidError> {
        if !(0.0..=0.125).contains(&capacity_m3) || self.cells.contains_key(&key) {
            return Err(FluidError::Invalid);
        }
        if self.cells.len() >= self.config.max_active_cells {
            return Err(FluidError::Capacity);
        }
        let next = self.next_revision()?;
        self.cells.insert(
            key,
            CellWater {
                capacity_m3,
                volume_m3: 0.0,
                positive_velocity_m_s: [0.0; 3],
                negative_boundary_velocity_m_s: [0.0; 3],
            },
        );
        self.revision = next;
        Ok(())
    }

    pub fn activate_from_geometry<G: crate::FluidGeometry>(
        &mut self,
        geometry: &G,
        key: [i32; 3],
    ) -> Result<(), FluidError> {
        let capacity = geometry.capacity_m3(key).ok_or(FluidError::Unavailable)?;
        self.activate(key, capacity)
    }

    pub fn set_capacity(&mut self, key: [i32; 3], capacity: f64) -> Result<(), FluidError> {
        if !(0.0..=0.125).contains(&capacity) {
            return Err(FluidError::Invalid);
        }
        let next = self.next_revision()?;
        let cell = self.cells.get_mut(&key).ok_or(FluidError::Unavailable)?;
        if cell.volume_m3 > capacity {
            return Err(FluidError::DisplacementRequired);
        }
        cell.capacity_m3 = capacity;
        self.revision = next;
        Ok(())
    }

    fn next_revision(&self) -> Result<u64, FluidError> {
        self.revision
            .checked_add(1)
            .ok_or(FluidError::RevisionExhausted)
    }
}
