use crate::{FluidError, FluidFace, FluidGeometry, WaterEndpoint};
use std::collections::BTreeMap;

#[derive(Clone, Copy)]
struct CellGeometry {
    capacity_m3: f64,
    faces: [FluidFace; 6],
}

/// Snapshot of host geometry for the active cells in one fluid step.
pub(super) struct PreparedFluidGeometry {
    cells: BTreeMap<[i32; 3], CellGeometry>,
}

impl PreparedFluidGeometry {
    pub(super) fn new<G: FluidGeometry>(
        geometry: &G,
        cells: impl IntoIterator<Item = [i32; 3]>,
    ) -> Result<Self, FluidError> {
        let mut prepared = BTreeMap::new();
        for cell in cells {
            let capacity_m3 = geometry.capacity_m3(cell).ok_or(FluidError::Unavailable)?;
            let mut faces = [FluidFace {
                open_area_m2: 0.0,
                boundary: WaterEndpoint::Solid,
            }; 6];
            for axis in 0..3 {
                for (side, positive) in [false, true].into_iter().enumerate() {
                    faces[axis * 2 + side] = geometry
                        .face(cell, axis, positive)
                        .ok_or(FluidError::Unavailable)?;
                }
            }
            prepared.insert(cell, CellGeometry { capacity_m3, faces });
        }
        Ok(Self { cells: prepared })
    }
}

impl FluidGeometry for PreparedFluidGeometry {
    fn capacity_m3(&self, cell: [i32; 3]) -> Option<f64> {
        self.cells.get(&cell).map(|geometry| geometry.capacity_m3)
    }

    fn face(&self, cell: [i32; 3], axis: usize, positive: bool) -> Option<FluidFace> {
        if axis >= 3 {
            return None;
        }
        let side = usize::from(positive);
        self.cells
            .get(&cell)
            .map(|geometry| geometry.faces[axis * 2 + side])
    }
}
