use crate::{FluidError, FluidSnapshot};

mod flux;
mod geometry_change;
mod prepared_geometry;
mod step;
mod surface;
mod water_read;

pub use step::FluidSnapshotExt;

pub const CELL_SIZE_M: f64 = 0.5;
pub const MAX_CFL: f64 = 0.5;
const CELL_VOLUME_M3: f64 = CELL_SIZE_M * CELL_SIZE_M * CELL_SIZE_M;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FluidStepReport {
    pub substeps: u32,
    pub transferred_m3: f64,
    pub active_cells: usize,
}

#[derive(Clone, Debug)]
pub struct GeometryChange {
    pub cell: [i32; 3],
    pub old_capacity_m3: f64,
    pub new_capacity_m3: f64,
    pub displaced_m3: f64,
    receivers: Vec<([i32; 3], f64)>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FluidInterfacePlane {
    pub cell: [i32; 3],
    pub normal: [f64; 3],
    pub offset_m: f64,
}
