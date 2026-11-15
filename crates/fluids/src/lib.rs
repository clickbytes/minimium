//! Pure water state and conservative exchange. Adapters own publication and I/O.
mod geometry;
mod plic;
mod pressure;
mod reservoir;
mod solver;
mod state;

pub use geometry::TerrainFluidGeometry;
pub use reservoir::{Reservoir, ReservoirKind, VolumeLevel};
pub use solver::{
    FluidInterfacePlane, FluidSnapshotExt, FluidStepReport, GeometryChange, CELL_SIZE_M, MAX_CFL,
};
pub use state::{CellWater, FluidConfig, FluidError, FluidSnapshot, WaterEndpoint};

/// Terrain-derived free volume and apertures, in the same 0.5 m lattice.
/// Unknown neighbors must be prepared or have an explicit reservoir boundary.
pub trait FluidGeometry: Send + Sync {
    fn capacity_m3(&self, cell: [i32; 3]) -> Option<f64>;
    fn face(&self, cell: [i32; 3], axis: usize, positive: bool) -> Option<FluidFace>;
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FluidFace {
    pub open_area_m2: f64,
    pub boundary: WaterEndpoint,
}
