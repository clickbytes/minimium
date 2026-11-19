//! Owner-lane water state and geometry handoff for excavation mutations.
mod edits;
mod frontier;
mod geometry;

use geometry::{
    active_cell_water_at_level, snapshots_quiet, CatalogWaterGeometry, CatalogWaterRead,
};
use minimium_fluids::{
    FluidConfig, FluidError, FluidGeometry, FluidSnapshot, FluidSnapshotExt, FluidStepReport,
    Reservoir, ReservoirKind, TerrainFluidGeometry, VolumeLevel, CELL_SIZE_M,
};
use minimium_procgen::WaterCatalog;
use minimium_world::world::WorldInstanceId;
use std::sync::Arc;

#[derive(Clone)]
pub(super) struct WaterWorld {
    snapshot: FluidSnapshot,
    catalog: Option<Arc<WaterCatalog>>,
    last_persisted_revision: u64,
    wake_pending: bool,
    quiet_steps: u8,
    reserved_cell_limit: usize,
}
pub(super) struct PreparedWaterEdit {
    snapshot: FluidSnapshot,
}

impl PreparedWaterEdit {
    pub(super) fn snapshot(&self) -> &FluidSnapshot {
        &self.snapshot
    }
}

impl WaterWorld {
    pub(super) fn new(world: WorldInstanceId, config: FluidConfig) -> Result<Self, FluidError> {
        Ok(Self {
            snapshot: FluidSnapshot::new(world, config)?,
            catalog: None,
            last_persisted_revision: 0,
            wake_pending: true,
            quiet_steps: 0,
            reserved_cell_limit: config.max_active_cells,
        })
    }

    pub(super) fn with_restored_snapshot(
        snapshot: FluidSnapshot,
        catalog: Arc<WaterCatalog>,
    ) -> Self {
        let last_persisted_revision = snapshot.revision();
        let reserved_cell_limit = snapshot.config().max_active_cells;
        Self {
            snapshot,
            catalog: Some(catalog),
            last_persisted_revision,
            wake_pending: true,
            quiet_steps: 0,
            reserved_cell_limit,
        }
    }

    /// Registers catalog reservoirs once while retaining deterministic IDs and curves.
    pub(super) fn from_catalog(
        world: WorldInstanceId,
        config: FluidConfig,
        catalog: &WaterCatalog,
        _ocean_level_m: f64,
    ) -> Result<Self, FluidError> {
        if !_ocean_level_m.is_finite() {
            return Err(FluidError::Invalid);
        }
        let mut result = Self::new(world, config)?;
        for body in &catalog.bodies {
            let reservoir = if body.ocean {
                Reservoir {
                    id: body.id,
                    volume_correction_m3: 0.0,
                    kind: ReservoirKind::Ocean {
                        level_m: body.initial_level_m,
                        exchanged_m3: 0.0,
                    },
                }
            } else {
                let curve: Vec<_> = body
                    .curve
                    .iter()
                    .map(|point| VolumeLevel {
                        level_m: point.level_m,
                        volume_m3: point.volume_m3,
                    })
                    .collect();
                if curve.len() < 2 {
                    return Err(FluidError::Invalid);
                }
                let volume_m3 = body.initial_volume_m3;
                Reservoir {
                    id: body.id,
                    volume_correction_m3: 0.0,
                    kind: ReservoirKind::Finite { curve, volume_m3 },
                }
            };
            result.snapshot.register_reservoir(reservoir)?;
        }
        result.catalog = Some(Arc::new(catalog.clone()));
        Ok(result)
    }

    pub(super) fn snapshot(&self) -> &FluidSnapshot {
        &self.snapshot
    }
    pub(super) fn snapshot_arc(&self) -> Arc<FluidSnapshot> {
        Arc::new(self.snapshot.clone())
    }
    pub(super) fn mark_persisted(&mut self, revision: u64) {
        self.last_persisted_revision = self
            .last_persisted_revision
            .max(revision.min(self.snapshot.revision()));
    }
    pub(super) fn dirty(&self) -> bool {
        self.last_persisted_revision != self.snapshot.revision()
    }

    pub(super) fn persisted_revision(&self) -> u64 {
        self.last_persisted_revision
    }

    pub(super) fn reserve_cell_limit(&mut self, limit: usize) -> Result<(), FluidError> {
        if limit > self.snapshot.config().max_active_cells || limit < self.snapshot.cells().len() {
            return Err(FluidError::Capacity);
        }
        self.reserved_cell_limit = limit;
        Ok(())
    }

    pub(super) fn reserved_cell_limit(&self) -> usize {
        self.reserved_cell_limit
    }

    pub(super) fn reconfigure(&mut self, config: FluidConfig) -> Result<(), FluidError> {
        if self.snapshot.config().max_active_cells == config.max_active_cells {
            return Ok(());
        }
        let revision = self
            .snapshot
            .revision()
            .checked_add(1)
            .ok_or(FluidError::RevisionExhausted)?;
        let replacement = FluidSnapshot::from_parts(
            self.snapshot.world(),
            config,
            revision,
            self.snapshot.cells().clone(),
            self.snapshot.reservoirs().clone(),
        )?;
        self.snapshot = replacement;
        self.reserved_cell_limit = config.max_active_cells;
        self.wake();
        Ok(())
    }

    /// No work is needed when the local velocity field and all reservoir
    /// exchanges are exactly stationary. Edits or boundary changes still wake it.
    pub(super) fn sleeping(&self) -> bool {
        !self.wake_pending
            && self.quiet_steps >= 3
            && self.snapshot.cells().values().all(|cell| {
                cell.positive_velocity_m_s
                    .iter()
                    .chain(cell.negative_boundary_velocity_m_s.iter())
                    .all(|velocity| velocity.abs() <= 1.0e-5)
            })
    }

    fn activate_with_external_water(
        &self,
        staged: &mut FluidSnapshot,
        cell: [i32; 3],
        capacity_m3: f64,
    ) -> Result<(), FluidError> {
        staged.activate(cell, capacity_m3)?;
        let Some(catalog) = &self.catalog else {
            return Ok(());
        };
        if capacity_m3 <= 0.0 {
            return Ok(());
        }
        let point = cell.map(|v| f64::from(v) * CELL_SIZE_M + CELL_SIZE_M * 0.5);
        let radius = point.iter().map(|v| v * v).sum::<f64>().sqrt();
        if radius <= f64::EPSILON {
            return Ok(());
        }
        let direction = point.map(|v| v / radius);
        let Some(ground_radius) = catalog.ground_radius_at_direction_m(direction) else {
            return Ok(());
        };
        if radius < ground_radius {
            return Ok(());
        }
        let Some(body) = catalog.body_at_direction(direction) else {
            return Ok(());
        };
        let Some(reservoir) = staged.reservoirs().get(&body) else {
            return Ok(());
        };
        let Ok(level) = reservoir.level_m() else {
            return Ok(());
        };
        let available = match &reservoir.kind {
            ReservoirKind::Finite { volume_m3, .. } => *volume_m3,
            ReservoirKind::Ocean { .. } => f64::INFINITY,
        };
        let requested = active_cell_water_at_level(catalog, body, cell, capacity_m3, level);
        let water = requested.min(available);
        if water > 0.0 {
            let curve_scale = water / requested;
            staged.transfer_from_reservoir_with_curve(body, cell, water, |curve_level| {
                active_cell_water_at_level(catalog, body, cell, capacity_m3, curve_level)
                    * curve_scale
            })?;
        }
        Ok(())
    }

    /// Preflights all 0.5 m cells covered by a brush against old and proposed
    /// terrain, using a private coherent clone so overlapping displacements reserve once.
    pub(super) fn prepare_terrain_edit(
        &self,
        old_terrain: &dyn minimium_world::terrain::TerrainRead,
        new_terrain: &dyn minimium_world::terrain::TerrainRead,
        request: minimium_terrain::TerrainEditRequest,
        server_cell_limit: usize,
    ) -> Result<PreparedWaterEdit, FluidError> {
        if request.center_m.iter().any(|v| !v.is_finite()) {
            return Err(FluidError::Invalid);
        }
        let radius_m = request.deformation.radius_m();
        let old_geometry = TerrainFluidGeometry::new(old_terrain);
        let new_geometry = TerrainFluidGeometry::new(new_terrain);
        let radius = radius_m + CELL_SIZE_M;
        let low = request
            .center_m
            .map(|v| ((v - radius) / CELL_SIZE_M).floor() as i32);
        let high = request
            .center_m
            .map(|v| ((v + radius) / CELL_SIZE_M).floor() as i32);
        let count: u64 = (0..3)
            .map(|axis| (i64::from(high[axis]) - i64::from(low[axis]) + 1).max(0) as u64)
            .product();
        if count > 65_536 || server_cell_limit == 0 || server_cell_limit > 524_288 {
            return Err(FluidError::Capacity);
        }
        let mut staged = self.snapshot.clone();
        for x in low[0]..=high[0] {
            for y in low[1]..=high[1] {
                for z in low[2]..=high[2] {
                    let cell = [x, y, z];
                    let old_capacity = old_geometry
                        .capacity_m3(cell)
                        .ok_or(FluidError::Unavailable)?;
                    let new_capacity = new_geometry
                        .capacity_m3(cell)
                        .ok_or(FluidError::Unavailable)?;
                    if (new_capacity - old_capacity).abs() <= 1.0e-9 {
                        continue;
                    }
                    if staged.cells().contains_key(&cell) {
                        let change =
                            staged.prepare_geometry_change(&old_geometry, cell, new_capacity)?;
                        staged.apply_geometry_change(change)?;
                    } else if new_capacity > 0.0 {
                        self.activate_with_external_water(&mut staged, cell, new_capacity)?;
                    }
                    if staged.cells().len() > self.reserved_cell_limit
                        || staged.cells().len() > server_cell_limit
                    {
                        return Err(FluidError::Capacity);
                    }
                }
            }
        }
        frontier::activate_frontier(self, &mut staged, &new_geometry, [0.0; 3], 9.81, 0.1)?;
        Ok(PreparedWaterEdit { snapshot: staged })
    }

    /// Craters inspect a bounded affected sphere so inactive lake cells are
    /// included in the same transaction as active water displacement.
    pub(super) fn prepare_crater_water(
        &self,
        old_terrain: &dyn minimium_world::terrain::TerrainRead,
        new_terrain: &dyn minimium_world::terrain::TerrainRead,
        crater: minimium_world::terrain_edits::CraterStamp,
        server_cell_limit: usize,
    ) -> Result<PreparedWaterEdit, FluidError> {
        edits::prepare_crater_water(self, old_terrain, new_terrain, crater, server_cell_limit)
    }

    pub(super) fn publish_terrain_edit(&mut self, prepared: PreparedWaterEdit) {
        self.snapshot = prepared.snapshot;
        self.wake();
    }

    pub(super) fn wake(&mut self) {
        self.wake_pending = true;
        self.quiet_steps = 0;
    }

    pub(super) fn step_terrain(
        &mut self,
        terrain: &dyn minimium_world::terrain::TerrainRead,
        center_m: [f64; 3],
        gravity_m_s2: f64,
        dt: f64,
    ) -> Result<FluidStepReport, FluidError> {
        let terrain_geometry = TerrainFluidGeometry::new(terrain);
        let mut staged = self.snapshot.clone();
        let activated = frontier::activate_frontier(
            self,
            &mut staged,
            &terrain_geometry,
            center_m,
            gravity_m_s2,
            dt,
        )?;
        let geometry_snapshot = staged.clone();
        let geometry = CatalogWaterGeometry {
            terrain: &terrain_geometry,
            catalog: self.catalog.as_deref(),
            snapshot: &geometry_snapshot,
            center_m,
        };
        let baseline = staged.clone();
        let result = staged.step_radial(&geometry, center_m, gravity_m_s2, dt);
        if let Ok(report) = result {
            self.snapshot = staged;
            self.wake_pending = false;
            if activated == 0
                && report.transferred_m3 <= 1.0e-10
                && snapshots_quiet(&baseline, &self.snapshot)
            {
                self.quiet_steps = self.quiet_steps.saturating_add(1);
            } else {
                self.quiet_steps = 0;
            }
        }
        result
    }

    fn external_water_body(
        &self,
        snapshot: &FluidSnapshot,
        cell: [i32; 3],
        center_m: [f64; 3],
    ) -> Option<minimium_world::fluid::WaterBodyId> {
        // This is only an edit-preparation candidate query. Actual cell water
        // is sampled against the base terrain before transfer.
        let point = cell.map(|value| f64::from(value) * CELL_SIZE_M + CELL_SIZE_M * 0.5);
        let catalog = self.catalog.as_ref()?;
        let radial: [f64; 3] = std::array::from_fn(|axis| point[axis] - center_m[axis]);
        let radius = radial.iter().map(|value| value * value).sum::<f64>().sqrt();
        if radius <= f64::EPSILON {
            return None;
        }
        let direction = radial.map(|value| value / radius);
        let id = catalog.body_at_direction(direction)?;
        let ground = catalog.ground_radius_at_direction_m(direction)?;
        let level = snapshot.reservoirs().get(&id)?.level_m().ok()?;
        let half_diagonal = CELL_SIZE_M * 3.0_f64.sqrt() * 0.5;
        (radius + half_diagonal >= ground && radius - half_diagonal <= level).then_some(id)
    }

    fn external_water_body_at_point(
        &self,
        snapshot: &FluidSnapshot,
        point: [f64; 3],
        center_m: [f64; 3],
    ) -> Option<minimium_world::fluid::WaterBodyId> {
        let catalog = self.catalog.as_ref()?;
        let radial: [f64; 3] = std::array::from_fn(|axis| point[axis] - center_m[axis]);
        let radius = radial.iter().map(|value| value * value).sum::<f64>().sqrt();
        if radius <= f64::EPSILON {
            return None;
        }
        let direction = radial.map(|value| value / radius);
        let id = catalog.body_at_direction(direction)?;
        let ground = catalog.ground_radius_at_direction_m(direction)?;
        let level = snapshot.reservoirs().get(&id)?.level_m().ok()?;
        (radius >= ground && radius <= level).then_some(id)
    }

    pub(super) fn water_read(&self) -> Arc<dyn minimium_world::fluid::WaterRead> {
        match &self.catalog {
            Some(catalog) => Arc::new(CatalogWaterRead::new(
                Arc::new(self.snapshot.clone()),
                Arc::clone(catalog),
            )),
            None => Arc::new(self.snapshot.clone()),
        }
    }
}
