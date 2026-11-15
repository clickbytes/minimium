//! Worker inputs and mesh preparation for authoritative water snapshots.
use std::sync::Arc;

#[derive(Clone)]
pub(in crate::renderer) struct WaterPresentation {
    pub catalog: Arc<minimium_procgen::WaterCatalog>,
    pub snapshot: Arc<minimium_fluids::FluidSnapshot>,
    pub level_revision: u64,
    pub center_m: [f64; 3],
    pub fluid_revision: u64,
    pub interface_boxes: Arc<Vec<[[f64; 3]; 2]>>,
    source_revision: u64,
}

impl WaterPresentation {
    pub fn new(
        catalog: Arc<minimium_procgen::WaterCatalog>,
        snapshot: Arc<minimium_fluids::FluidSnapshot>,
    ) -> Self {
        Self {
            catalog,
            snapshot,
            level_revision: 0,
            center_m: [0.0; 3],
            fluid_revision: 0,
            interface_boxes: Arc::new(Vec::new()),
            source_revision: u64::MAX,
        }
    }

    pub fn set_center(&mut self, center_m: [f64; 3]) {
        let center_changed = self.center_m != center_m;
        self.center_m = center_m;
        if !center_changed && self.source_revision == self.snapshot.revision() {
            return;
        }
        self.update_revision();
    }

    fn update_revision(&mut self) {
        let mut revision = 0xcbf2_9ce4_8422_2325_u64;
        for byte in self.catalog.digest {
            revision = (revision ^ u64::from(byte)).wrapping_mul(0x100_0000_01b3);
        }
        for (id, reservoir) in self.snapshot.reservoirs() {
            let level = reservoir.level_m().unwrap_or(f64::NAN);
            for value in [id.0, level.to_bits()] {
                for byte in value.to_le_bytes() {
                    revision = (revision ^ u64::from(byte)).wrapping_mul(0x100_0000_01b3);
                }
            }
        }
        let interface_cells = self
            .snapshot
            .interface_cells_near(self.center_m, 96.0, 4096);
        self.interface_boxes = Arc::new(
            interface_cells
                .iter()
                .map(|cell| {
                    let minimum = cell.map(|value| f64::from(value) * 0.5);
                    [minimum, minimum.map(|value| value + 0.5)]
                })
                .collect(),
        );
        for cell in interface_cells {
            for coordinate in cell {
                for byte in coordinate.to_le_bytes() {
                    revision = (revision ^ u64::from(byte)).wrapping_mul(0x100_0000_01b3);
                }
            }
        }
        self.level_revision = revision;
        self.fluid_revision = self.snapshot.revision();
        self.source_revision = self.snapshot.revision();
    }
}

pub(super) fn build_water_vertices(
    water: Option<&WaterPresentation>,
    origin: [f32; 3],
) -> Vec<minimium_renderer::ocean_geometry::OceanVertex> {
    let Some(water) = water else {
        return Vec::new();
    };
    let origin = origin.map(f64::from);
    water
        .snapshot
        .interface_planes_near(water.center_m, 96.0, 4096)
        .into_iter()
        .flat_map(|plane| {
            let center = plane.cell.map(|value| f64::from(value) * 0.5 + 0.25);
            let flow = water
                .snapshot
                .sample_cell(plane.cell)
                .map_or([0.0; 3], |sample| sample.velocity_m_s.map(|v| v as f32));
            minimium_renderer::water_surface::plane_triangles(
                minimium_renderer::water_surface::WaterInterfacePlane {
                    cell_center_m: center,
                    normal: plane.normal,
                    offset_m: plane.offset_m,
                    flow_m_s: flow,
                },
            )
            .into_iter()
            .map(|vertex| minimium_renderer::ocean_geometry::OceanVertex {
                position: std::array::from_fn(|axis| {
                    (vertex.position_m[axis] - origin[axis]) as f32
                }),
                normal: vertex.normal,
            })
            .collect::<Vec<_>>()
        })
        .collect()
}
