use crate::FluidError;
use minimium_world::fluid::WaterBodyId;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VolumeLevel {
    pub level_m: f64,
    pub volume_m3: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ReservoirKind {
    Finite {
        curve: Vec<VolumeLevel>,
        volume_m3: f64,
    },
    /// Positive exchange is water supplied by the ocean to the local system.
    Ocean { level_m: f64, exchanged_m3: f64 },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Reservoir {
    pub id: WaterBodyId,
    pub kind: ReservoirKind,
    /// Low-order residual paired with the quantity stored by `kind`.
    pub volume_correction_m3: f64,
}

impl Reservoir {
    pub fn valid(&self) -> bool {
        self.id.0 != 0
            && self.volume_correction_m3.is_finite()
            && match &self.kind {
                ReservoirKind::Ocean {
                    level_m,
                    exchanged_m3,
                } => {
                    level_m.is_finite()
                        && exchanged_m3.is_finite()
                        && two_sum(*exchanged_m3, self.volume_correction_m3).is_some()
                }
                ReservoirKind::Finite { curve, volume_m3 } => {
                    (2..=4096).contains(&curve.len())
                        && curve[0].volume_m3 == 0.0
                        && curve
                            .iter()
                            .all(|p| p.level_m.is_finite() && p.volume_m3.is_finite())
                        && curve.windows(2).all(|p| {
                            p[0].level_m < p[1].level_m && p[0].volume_m3 <= p[1].volume_m3
                        })
                        && pair_in_range(
                            *volume_m3,
                            self.volume_correction_m3,
                            0.0,
                            curve.last().unwrap().volume_m3,
                        )
                }
            }
    }

    pub fn level_m(&self) -> Result<f64, FluidError> {
        if !self.valid() {
            return Err(FluidError::Invalid);
        }
        match &self.kind {
            ReservoirKind::Ocean { level_m, .. } => Ok(*level_m),
            ReservoirKind::Finite { curve, volume_m3 } => {
                let volume = *volume_m3 + self.volume_correction_m3;
                if volume <= 0.0 {
                    return Ok(curve[0].level_m);
                }
                let upper = curve.partition_point(|point| point.volume_m3 <= volume);
                if upper == 0 {
                    return Err(FluidError::Invalid);
                }
                if upper == curve.len() {
                    return Ok(curve.last().unwrap().level_m);
                }
                let a = curve[upper - 1];
                let b = curve[upper];
                if a.volume_m3 == volume {
                    return Ok(a.level_m);
                }
                let fraction = (volume - a.volume_m3) / (b.volume_m3 - a.volume_m3);
                Ok(a.level_m + fraction * (b.level_m - a.level_m))
            }
        }
    }

    pub(crate) fn available(&self) -> f64 {
        match self.kind {
            ReservoirKind::Finite { volume_m3, .. } => volume_m3 + self.volume_correction_m3,
            ReservoirKind::Ocean { .. } => f64::INFINITY,
        }
    }

    pub(crate) fn capacity(&self) -> f64 {
        match &self.kind {
            ReservoirKind::Finite { volume_m3, curve } => {
                curve.last().unwrap().volume_m3 - volume_m3 - self.volume_correction_m3
            }
            ReservoirKind::Ocean { .. } => f64::INFINITY,
        }
    }

    pub(crate) fn can_add_water(&self, amount: f64) -> bool {
        if !amount.is_finite() {
            return false;
        }
        let (high, delta) = match self.kind {
            ReservoirKind::Finite { volume_m3, .. } => (volume_m3, amount),
            ReservoirKind::Ocean { exchanged_m3, .. } => (exchanged_m3, -amount),
        };
        let Some((next, correction)) = add_pair(high, self.volume_correction_m3, delta) else {
            return false;
        };
        match &self.kind {
            ReservoirKind::Finite { curve, .. } => {
                pair_in_range(next, correction, 0.0, curve.last().unwrap().volume_m3)
            }
            ReservoirKind::Ocean { .. } => true,
        }
    }

    pub(crate) fn add(&mut self, amount: f64) -> bool {
        if !self.can_add_water(amount) {
            return false;
        }
        let (high, delta) = match &mut self.kind {
            ReservoirKind::Finite { volume_m3, .. } => (volume_m3, amount),
            ReservoirKind::Ocean { exchanged_m3, .. } => (exchanged_m3, -amount),
        };
        let Some((next, correction)) = add_pair(*high, self.volume_correction_m3, delta) else {
            return false;
        };
        *high = next;
        self.volume_correction_m3 = correction;
        true
    }

    pub(crate) fn quantity_parts(&self) -> (f64, f64) {
        match self.kind {
            ReservoirKind::Finite { volume_m3, .. } => (volume_m3, self.volume_correction_m3),
            ReservoirKind::Ocean { exchanged_m3, .. } => {
                (-exchanged_m3, -self.volume_correction_m3)
            }
        }
    }

    pub(crate) fn debit_ceiling(high: f64, low: f64, debit: f64) -> Option<f64> {
        let (high, low) = add_pair(high, low, -debit)?;
        let (rounded, error) = two_sum(high, low)?;
        if error > 0.0 {
            Some(next_up_nonnegative(rounded))
        } else {
            Some(rounded)
        }
    }
}

fn two_sum(a: f64, b: f64) -> Option<(f64, f64)> {
    let sum = a + b;
    if !sum.is_finite() {
        return None;
    }
    let b_virtual = sum - a;
    let error = (a - (sum - b_virtual)) + (b - b_virtual);
    Some((sum, error))
}

fn add_pair(high: f64, low: f64, delta: f64) -> Option<(f64, f64)> {
    let (sum, error) = two_sum(high, delta)?;
    let (sum, low_error) = two_sum(sum, low)?;
    let (residual, residual_error) = two_sum(error, low_error)?;
    let (high, carry) = two_sum(sum, residual)?;
    let (low, tail) = two_sum(carry, residual_error)?;
    let (high, correction) = two_sum(high, low)?;
    Some((high, correction + tail))
}

fn pair_cmp_bound(high: f64, low: f64, bound: f64) -> std::cmp::Ordering {
    let (rounded, error) = two_sum(high, low).unwrap_or((high + low, 0.0));
    rounded
        .total_cmp(&bound)
        .then_with(|| error.total_cmp(&0.0))
}

fn pair_in_range(high: f64, low: f64, minimum: f64, maximum: f64) -> bool {
    pair_cmp_bound(high, low, minimum) != std::cmp::Ordering::Less
        && pair_cmp_bound(high, low, maximum) != std::cmp::Ordering::Greater
}

fn next_up_nonnegative(value: f64) -> f64 {
    if value == f64::INFINITY {
        value
    } else if value == 0.0 {
        f64::from_bits(1)
    } else {
        f64::from_bits(value.to_bits() + 1)
    }
}
