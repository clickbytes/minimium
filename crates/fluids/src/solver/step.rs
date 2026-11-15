use super::flux::{endpoint_wet, face_point, limit_reservoir_outflows};
use super::prepared_geometry::PreparedFluidGeometry;
use super::*;
use crate::{FluidGeometry, WaterEndpoint};
use std::collections::BTreeMap;

/// CPU local-water operations with bounded Jacobi pressure projection and
/// sampled geometric plane reconstruction for conservative face transport.
pub trait FluidSnapshotExt {
    fn prepare_geometry_change<G: FluidGeometry>(
        &self,
        geometry: &G,
        cell: [i32; 3],
        new_capacity_m3: f64,
    ) -> Result<GeometryChange, FluidError>;

    fn apply_geometry_change(&mut self, change: GeometryChange) -> Result<(), FluidError>;

    fn step<G: FluidGeometry>(
        &mut self,
        geometry: &G,
        gravity_m_s2: [f64; 3],
        dt: f64,
    ) -> Result<FluidStepReport, FluidError>;

    fn step_with_gravity_field<G, F>(
        &mut self,
        geometry: &G,
        gravity_at_cell: F,
        dt: f64,
    ) -> Result<FluidStepReport, FluidError>
    where
        G: FluidGeometry,
        F: Fn([i32; 3]) -> [f64; 3];

    fn step_field<G, F>(
        &mut self,
        geometry: &G,
        gravity_at_cell: F,
        radial_center: Option<[f64; 3]>,
        dt: f64,
    ) -> Result<FluidStepReport, FluidError>
    where
        G: FluidGeometry,
        F: Fn([i32; 3]) -> [f64; 3];

    fn step_radial<G: FluidGeometry>(
        &mut self,
        geometry: &G,
        center_m: [f64; 3],
        gravity_m_s2: f64,
        dt: f64,
    ) -> Result<FluidStepReport, FluidError>;
}

impl FluidSnapshotExt for FluidSnapshot {
    fn prepare_geometry_change<G: FluidGeometry>(
        &self,
        geometry: &G,
        cell: [i32; 3],
        new_capacity_m3: f64,
    ) -> Result<GeometryChange, FluidError> {
        geometry_change::prepare(self, geometry, cell, new_capacity_m3)
    }

    fn apply_geometry_change(&mut self, change: GeometryChange) -> Result<(), FluidError> {
        geometry_change::apply(self, change)
    }

    fn step<G: FluidGeometry>(
        &mut self,
        geometry: &G,
        gravity: [f64; 3],
        dt: f64,
    ) -> Result<FluidStepReport, FluidError> {
        self.step_with_gravity_field(geometry, |_| gravity, dt)
    }

    fn step_with_gravity_field<G, F>(
        &mut self,
        geometry: &G,
        gravity_at_cell: F,
        dt: f64,
    ) -> Result<FluidStepReport, FluidError>
    where
        G: FluidGeometry,
        F: Fn([i32; 3]) -> [f64; 3],
    {
        self.step_field(geometry, gravity_at_cell, None, dt)
    }

    fn step_field<G, F>(
        &mut self,
        geometry: &G,
        gravity_at_cell: F,
        radial_center: Option<[f64; 3]>,
        dt: f64,
    ) -> Result<FluidStepReport, FluidError>
    where
        G: FluidGeometry,
        F: Fn([i32; 3]) -> [f64; 3],
    {
        if !dt.is_finite() || dt <= 0.0 {
            return Err(FluidError::Invalid);
        }
        let keys: Vec<_> = self.cells().keys().copied().collect();
        let prepared_geometry = PreparedFluidGeometry::new(geometry, keys.iter().copied())?;
        let geometry = &prepared_geometry;
        let mut staged = self.clone();
        let mut max_cfl: f64 = 0.0;
        for key in &keys {
            let cell = staged.cells().get(key).ok_or(FluidError::Unavailable)?;
            let gravity = gravity_at_cell(*key);
            if gravity.iter().any(|v| !v.is_finite()) {
                return Err(FluidError::Invalid);
            }
            let mut outgoing_rate = 0.0;
            for axis in 0..3 {
                let face = geometry
                    .face(*key, axis, true)
                    .ok_or(FluidError::Unavailable)?;
                if face.open_area_m2 > 0.0 && cell.capacity_m3 > 0.0 {
                    let velocity =
                        cell.positive_velocity_m_s[axis].abs() + gravity[axis].abs() * dt;
                    outgoing_rate += velocity * face.open_area_m2;
                }
                let negative = geometry
                    .face(*key, axis, false)
                    .ok_or(FluidError::Unavailable)?;
                if negative.open_area_m2 > 0.0 && cell.capacity_m3 > 0.0 {
                    let velocity = match negative.boundary {
                        WaterEndpoint::Cell(prev) => {
                            staged
                                .cells()
                                .get(&prev)
                                .ok_or(FluidError::Unavailable)?
                                .positive_velocity_m_s[axis]
                        }
                        _ => cell.negative_boundary_velocity_m_s[axis],
                    };
                    outgoing_rate +=
                        (velocity.abs() + gravity[axis].abs() * dt) * negative.open_area_m2;
                }
            }
            if cell.capacity_m3 > 0.0 {
                max_cfl = max_cfl.max(outgoing_rate * dt / cell.capacity_m3);
            }
        }
        let required = (max_cfl / MAX_CFL).ceil();
        if !required.is_finite() || required > 4096.0 {
            return Err(FluidError::Capacity);
        }
        let initial_substeps = (required as u32).max(1);
        let mut h = dt / f64::from(initial_substeps);
        let mut remaining = dt;
        let mut substeps = 0_u32;
        let mut moved = 0.0;
        let mut retries = 0_u32;
        while remaining > dt * 1.0e-12 {
            if substeps >= 4096 {
                return Err(FluidError::Capacity);
            }
            h = h.min(remaining);
            let before_substep = staged.clone();
            // Gravity accelerates each MAC positive-face component. Missing faces
            // are explicit boundaries; no absent neighbor is treated as empty space.
            let mut cfl_violation = false;
            for key in &keys {
                let mut cell = staged
                    .cells()
                    .get(key)
                    .copied()
                    .ok_or(FluidError::Unavailable)?;
                let gravity = gravity_at_cell(*key);
                if gravity.iter().any(|v| !v.is_finite()) {
                    return Err(FluidError::Invalid);
                }
                for axis in 0..3 {
                    let face = geometry
                        .face(*key, axis, true)
                        .ok_or(FluidError::Unavailable)?;
                    let point = face_point(*key, axis, true);
                    let cell_wet =
                        endpoint_wet(&staged, WaterEndpoint::Cell(*key), point, radial_center)?;
                    let boundary_wet = endpoint_wet(&staged, face.boundary, point, radial_center)?;
                    if face.open_area_m2 > 0.0 && (cell_wet || boundary_wet) {
                        cell.positive_velocity_m_s[axis] += gravity[axis] * h;
                    } else {
                        cell.positive_velocity_m_s[axis] = 0.0;
                    }
                    let negative = geometry
                        .face(*key, axis, false)
                        .ok_or(FluidError::Unavailable)?;
                    if negative.open_area_m2 > 0.0
                        && !matches!(negative.boundary, WaterEndpoint::Cell(_))
                        && (cell_wet
                            || endpoint_wet(
                                &staged,
                                negative.boundary,
                                face_point(*key, axis, false),
                                radial_center,
                            )?)
                    {
                        cell.negative_boundary_velocity_m_s[axis] += gravity[axis] * h;
                    } else {
                        cell.negative_boundary_velocity_m_s[axis] = 0.0;
                    }
                }
                staged.replace_face_velocities(
                    *key,
                    cell.positive_velocity_m_s,
                    cell.negative_boundary_velocity_m_s,
                )?;
            }
            let projected =
                crate::pressure::project(&staged, geometry, h, radial_center, &gravity_at_cell)?;
            for (key, (positive, negative)) in projected {
                staged.replace_face_velocities(key, positive, negative)?;
            }
            for key in &keys {
                let cell = staged.cells().get(key).ok_or(FluidError::Unavailable)?;
                let mut outgoing_rate = 0.0;
                let mut incoming_rate = 0.0;
                for axis in 0..3 {
                    let positive = geometry
                        .face(*key, axis, true)
                        .ok_or(FluidError::Unavailable)?;
                    let positive_velocity = cell.positive_velocity_m_s[axis];
                    outgoing_rate += positive_velocity.max(0.0) * positive.open_area_m2;
                    incoming_rate += (-positive_velocity).max(0.0) * positive.open_area_m2;
                    let negative = geometry
                        .face(*key, axis, false)
                        .ok_or(FluidError::Unavailable)?;
                    let value = match negative.boundary {
                        WaterEndpoint::Cell(prev) => {
                            staged
                                .cells()
                                .get(&prev)
                                .ok_or(FluidError::Unavailable)?
                                .positive_velocity_m_s[axis]
                        }
                        _ => cell.negative_boundary_velocity_m_s[axis],
                    };
                    outgoing_rate += (-value).max(0.0) * negative.open_area_m2;
                    incoming_rate += value.max(0.0) * negative.open_area_m2;
                }
                if cell.capacity_m3 > 0.0 && outgoing_rate * h / cell.capacity_m3 > MAX_CFL + 1.0e-9
                {
                    cfl_violation = true;
                    break;
                }
                if cell.capacity_m3 > 0.0 && incoming_rate * h / cell.capacity_m3 > MAX_CFL + 1.0e-9
                {
                    cfl_violation = true;
                    break;
                }
            }
            if cfl_violation {
                staged = before_substep;
                retries += 1;
                h *= 0.5;
                if retries > 32 || h < dt / 8192.0 {
                    return Err(FluidError::Capacity);
                }
                continue;
            }
            let mut fluxes = Vec::new();
            let planes: BTreeMap<_, _> = staged
                .cells()
                .keys()
                .filter_map(|key| {
                    crate::plic::reconstruct(&staged, *key).map(|plane| (*key, plane))
                })
                .collect();
            for key in &keys {
                let source = *key;
                let cell = staged.cells()[&source];
                for axis in 0..3 {
                    let face = geometry
                        .face(source, axis, true)
                        .ok_or(FluidError::Unavailable)?;
                    let velocity = cell.positive_velocity_m_s[axis];
                    if face.open_area_m2 > 0.0
                        && !matches!(face.boundary, WaterEndpoint::Solid | WaterEndpoint::Air)
                        && velocity != 0.0
                    {
                        let route = if velocity > 0.0 {
                            Some((
                                WaterEndpoint::Cell(source),
                                face.boundary,
                                Some(source),
                                cell.volume_m3,
                            ))
                        } else {
                            match face.boundary {
                                WaterEndpoint::Cell(neighbor) => Some((
                                    face.boundary,
                                    WaterEndpoint::Cell(source),
                                    Some(neighbor),
                                    staged
                                        .cells()
                                        .get(&neighbor)
                                        .copied()
                                        .ok_or(FluidError::Unavailable)?
                                        .volume_m3,
                                )),
                                WaterEndpoint::Reservoir(id) => {
                                    let point = face_point(source, axis, true);
                                    if !endpoint_wet(&staged, face.boundary, point, radial_center)?
                                    {
                                        None
                                    } else {
                                        let available = staged
                                            .reservoirs()
                                            .get(&id)
                                            .ok_or(FluidError::Unavailable)?
                                            .available();
                                        Some((
                                            face.boundary,
                                            WaterEndpoint::Cell(source),
                                            None,
                                            available,
                                        ))
                                    }
                                }
                                WaterEndpoint::Solid | WaterEndpoint::Air => None,
                            }
                        };
                        if let Some((from, to, donor_key, available)) = route {
                            let phase = donor_key.and_then(|key| planes.get(&key).copied()).map_or(
                                1.0,
                                |plane| {
                                    crate::plic::swept_fraction(
                                        plane,
                                        axis,
                                        velocity > 0.0,
                                        velocity.abs() * h,
                                    )
                                },
                            );
                            let requested =
                                (velocity.abs() * face.open_area_m2 * h * phase).min(available);
                            if requested > 0.0 {
                                fluxes.push((from, to, requested));
                            }
                        }
                    }

                    let negative = geometry
                        .face(source, axis, false)
                        .ok_or(FluidError::Unavailable)?;
                    if negative.open_area_m2 > 0.0
                        && !matches!(negative.boundary, WaterEndpoint::Cell(_))
                        && !matches!(negative.boundary, WaterEndpoint::Air | WaterEndpoint::Solid)
                    {
                        let boundary_velocity = cell.negative_boundary_velocity_m_s[axis];
                        if boundary_velocity != 0.0 {
                            let (from, to, donor_key, available) = if boundary_velocity > 0.0 {
                                let point = face_point(source, axis, false);
                                let is_wet =
                                    endpoint_wet(&staged, negative.boundary, point, radial_center)?;
                                let WaterEndpoint::Reservoir(id) = negative.boundary else {
                                    return Err(FluidError::Unavailable);
                                };
                                let available = staged
                                    .reservoirs()
                                    .get(&id)
                                    .ok_or(FluidError::Unavailable)?
                                    .available()
                                    .min(if is_wet { f64::INFINITY } else { 0.0 });
                                (
                                    negative.boundary,
                                    WaterEndpoint::Cell(source),
                                    None,
                                    available,
                                )
                            } else {
                                (
                                    WaterEndpoint::Cell(source),
                                    negative.boundary,
                                    Some(source),
                                    cell.volume_m3,
                                )
                            };
                            let phase = donor_key.and_then(|key| planes.get(&key).copied()).map_or(
                                1.0,
                                |plane| {
                                    crate::plic::swept_fraction(
                                        plane,
                                        axis,
                                        false,
                                        boundary_velocity.abs() * h,
                                    )
                                },
                            );
                            let requested =
                                (boundary_velocity.abs() * negative.open_area_m2 * h * phase)
                                    .min(available);
                            if requested > 0.0 {
                                fluxes.push((from, to, requested));
                            }
                        }
                    }
                }
            }
            limit_reservoir_outflows(&staged, &mut fluxes)?;
            if let Err(error) = staged.transfer_batch(&fluxes) {
                if error != FluidError::InsufficientVolume {
                    return Err(error);
                }
                staged = before_substep;
                retries += 1;
                h *= 0.5;
                if retries > 32 || h < dt / 8192.0 {
                    return Err(FluidError::Capacity);
                }
                continue;
            }
            moved += fluxes.iter().map(|(_, _, volume)| volume).sum::<f64>();
            remaining = (remaining - h).max(0.0);
            substeps += 1;
            retries = 0;
        }
        let report = FluidStepReport {
            substeps,
            transferred_m3: moved,
            active_cells: staged.cells().len(),
        };
        *self = staged;
        Ok(report)
    }

    fn step_radial<G: FluidGeometry>(
        &mut self,
        geometry: &G,
        center: [f64; 3],
        gravity: f64,
        dt: f64,
    ) -> Result<FluidStepReport, FluidError> {
        if center.iter().any(|v| !v.is_finite()) || !gravity.is_finite() || gravity < 0.0 {
            return Err(FluidError::Invalid);
        }
        self.step_field(
            geometry,
            |key| {
                let point = key.map(|v| f64::from(v) * CELL_SIZE_M + CELL_SIZE_M * 0.5);
                let toward_center: [f64; 3] =
                    std::array::from_fn(|axis| center[axis] - point[axis]);
                let length = toward_center.iter().map(|v| v * v).sum::<f64>().sqrt();
                if length <= f64::EPSILON {
                    [0.0; 3]
                } else {
                    toward_center.map(|v| v * gravity / length)
                }
            },
            Some(center),
            dt,
        )
    }
}
