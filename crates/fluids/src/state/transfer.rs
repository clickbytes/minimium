use super::{FluidError, FluidSnapshot, WaterEndpoint};
use crate::Reservoir;
use std::collections::BTreeMap;

impl FluidSnapshot {
    /// Exact requested exchange or no change. Never clamps away excess water.
    pub fn transfer(
        &mut self,
        from: WaterEndpoint,
        to: WaterEndpoint,
        volume: f64,
    ) -> Result<(), FluidError> {
        if from == to || !volume.is_finite() || volume <= 0.0 {
            return Err(FluidError::Invalid);
        }
        if self.available(from)? < volume || self.capacity(to)? < volume {
            return Err(FluidError::InsufficientVolume);
        }
        let next = self.next_revision()?;
        for (endpoint, delta) in [(from, -volume), (to, volume)] {
            match endpoint {
                WaterEndpoint::Cell(_) => {
                    let current = self.quantity(endpoint)?;
                    if !(current + delta).is_finite() || current + delta == current {
                        return Err(FluidError::Invalid);
                    }
                }
                WaterEndpoint::Reservoir(id) => {
                    if !self.reservoirs.get(&id).unwrap().can_add_water(delta) {
                        return Err(FluidError::InsufficientVolume);
                    }
                }
                WaterEndpoint::Solid | WaterEndpoint::Air => unreachable!(),
            }
        }
        self.add(from, -volume);
        self.add(to, volume);
        self.revision = next;
        Ok(())
    }

    /// Commits a simultaneous conservative flux set after validating every endpoint.
    pub fn transfer_batch(
        &mut self,
        fluxes: &[(WaterEndpoint, WaterEndpoint, f64)],
    ) -> Result<(), FluidError> {
        let mut changes = BTreeMap::<WaterEndpoint, f64>::new();
        for (from, to, volume) in fluxes {
            if from == to || !volume.is_finite() || *volume <= 0.0 {
                return Err(FluidError::Invalid);
            }
            *changes.entry(*from).or_default() -= *volume;
            *changes.entry(*to).or_default() += *volume;
        }
        for (endpoint, delta) in &changes {
            if *delta == 0.0 {
                continue;
            }
            match endpoint {
                WaterEndpoint::Cell(_) => {
                    let current = self.quantity(*endpoint)?;
                    let next = current + delta;
                    if !next.is_finite() || next == current {
                        return Err(FluidError::InsufficientVolume);
                    }
                    let capacity = current + self.capacity(*endpoint)?;
                    if next < 0.0 || next > capacity {
                        return Err(FluidError::InsufficientVolume);
                    }
                }
                WaterEndpoint::Reservoir(id) => {
                    if !self
                        .reservoirs
                        .get(id)
                        .ok_or(FluidError::Unavailable)?
                        .can_add_water(*delta)
                    {
                        return Err(FluidError::InsufficientVolume);
                    }
                }
                WaterEndpoint::Solid | WaterEndpoint::Air => return Err(FluidError::Unavailable),
            }
        }
        if changes.is_empty() {
            return Ok(());
        }
        let revision = self.next_revision()?;
        for (endpoint, delta) in changes {
            self.add(endpoint, delta);
        }
        self.revision = revision;
        Ok(())
    }

    pub fn accounted_volume_m3(&self) -> f64 {
        let mut sum = 0.0;
        let mut correction = 0.0;
        let mut add = |value: f64| {
            let adjusted = value - correction;
            let next = sum + adjusted;
            correction = (next - sum) - adjusted;
            sum = next;
        };
        for cell in self.cells.values() {
            add(cell.volume_m3);
        }
        for reservoir in self.reservoirs.values() {
            let (high, low) = reservoir.quantity_parts();
            add(high);
            add(low);
        }
        sum
    }

    fn available(&self, at: WaterEndpoint) -> Result<f64, FluidError> {
        match at {
            WaterEndpoint::Cell(key) => self.cells.get(&key).map(|c| c.volume_m3),
            WaterEndpoint::Reservoir(id) => self.reservoirs.get(&id).map(Reservoir::available),
            WaterEndpoint::Solid | WaterEndpoint::Air => None,
        }
        .ok_or(FluidError::Unavailable)
    }

    fn capacity(&self, at: WaterEndpoint) -> Result<f64, FluidError> {
        match at {
            WaterEndpoint::Cell(key) => self.cells.get(&key).map(|c| c.capacity_m3 - c.volume_m3),
            WaterEndpoint::Reservoir(id) => self.reservoirs.get(&id).map(Reservoir::capacity),
            WaterEndpoint::Solid | WaterEndpoint::Air => None,
        }
        .ok_or(FluidError::Unavailable)
    }

    fn quantity(&self, at: WaterEndpoint) -> Result<f64, FluidError> {
        match at {
            WaterEndpoint::Cell(key) => self.cells.get(&key).map(|c| c.volume_m3),
            WaterEndpoint::Reservoir(id) => self.reservoirs.get(&id).map(|reservoir| {
                let (high, low) = reservoir.quantity_parts();
                high + low
            }),
            WaterEndpoint::Solid | WaterEndpoint::Air => None,
        }
        .ok_or(FluidError::Unavailable)
    }

    fn add(&mut self, at: WaterEndpoint, amount: f64) {
        match at {
            WaterEndpoint::Cell(key) => self.cells.get_mut(&key).unwrap().volume_m3 += amount,
            WaterEndpoint::Reservoir(id) => {
                assert!(self.reservoirs.get_mut(&id).unwrap().add(amount));
            }
            WaterEndpoint::Solid | WaterEndpoint::Air => {
                unreachable!("validated exchange endpoint")
            }
        }
    }
}
