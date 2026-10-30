use minimium_assets::{
    embedded_sky_catalog, embedded_sky_catalog_identity, embedded_star_labels, CatalogError,
    SkyCatalogIdentity, StarRecord,
};
use minimium_astronomy::{CorrectedStarDirections, StarAstrometry};
use std::{error::Error, fmt};

pub const STAR_GPU_RECORD_COUNT: usize = 1000;
pub const STAR_GPU_RECORD_BYTES: usize = 32;
pub const STAR_COLOR_MODEL_REVISION: u32 = 1;

const UNIT_DIRECTION_TOLERANCE: f64 = 1.0e-8;
const BV_COLOR_NODES: [(f32, [f32; 3]); 7] = [
    (-0.4, [0.60, 0.70, 1.00]),
    (0.0, [0.78, 0.85, 1.00]),
    (0.4, [0.92, 0.94, 1.00]),
    (0.65, [1.00, 0.98, 0.94]),
    (1.0, [1.00, 0.85, 0.70]),
    (1.5, [1.00, 0.69, 0.48]),
    (2.0, [1.00, 0.55, 0.34]),
];

/// One per-star GPU instance: EQJ direction/magnitude, then linear RGB/HYG ID.
#[repr(C, align(16))]
#[derive(Clone, Copy, Debug, Default, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct StarInstanceGpu {
    pub direction_eqj: [f32; 3],
    pub visual_magnitude: f32,
    pub color_linear: [f32; 3],
    pub catalog_star_id: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StarPrepareError {
    InvalidRecordCount { records: usize, directions: usize },
    InvalidEpochRevision,
    RecordMismatch { index: usize },
    InvalidAstrometry { index: usize },
}

impl fmt::Display for StarPrepareError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidRecordCount {
                records,
                directions,
            } => write!(
                formatter,
                "star instance count mismatch: {records} records, {directions} directions"
            ),
            Self::InvalidEpochRevision => {
                formatter.write_str("star direction epoch revision is zero")
            }
            Self::RecordMismatch { index } => {
                write!(
                    formatter,
                    "star direction ID does not match catalogue at {index}"
                )
            }
            Self::InvalidAstrometry { index } => {
                write!(
                    formatter,
                    "star astrometry is invalid at catalogue index {index}"
                )
            }
        }
    }
}

impl Error for StarPrepareError {}

#[allow(dead_code)]
pub struct StarCatalogueAdapter {
    identity: SkyCatalogIdentity,
    records: &'static [StarRecord],
    labels: &'static [&'static str],
    astrometry: Box<[StarAstrometry]>,
}

#[allow(dead_code)]
impl StarCatalogueAdapter {
    pub fn load_embedded() -> Result<Self, CatalogError> {
        let catalogue = embedded_sky_catalog()?;
        let astrometry = catalogue
            .records()
            .iter()
            .map(|record| StarAstrometry {
                id: record.hyg_id,
                position_eqj_pc: record.position_eqj_pc,
                velocity_eqj_pc_per_jyear: record.velocity_eqj_pc_per_jyear,
            })
            .collect::<Vec<_>>()
            .into_boxed_slice();
        Ok(Self {
            identity: embedded_sky_catalog_identity(),
            records: catalogue.records(),
            labels: embedded_star_labels()?,
            astrometry,
        })
    }

    pub fn identity(&self) -> SkyCatalogIdentity {
        self.identity
    }

    pub fn records(&self) -> &'static [StarRecord] {
        self.records
    }

    pub fn labels(&self) -> &'static [&'static str] {
        self.labels
    }

    pub fn astrometry(&self) -> &[StarAstrometry] {
        &self.astrometry
    }

    /// Builds the exact asset-ordered GPU batch from one matching CPU epoch.
    pub fn prepare_gpu_instances(
        &self,
        corrected: &CorrectedStarDirections,
    ) -> Result<Box<[StarInstanceGpu; STAR_GPU_RECORD_COUNT]>, StarPrepareError> {
        if corrected.star_epoch_revision == 0 {
            return Err(StarPrepareError::InvalidEpochRevision);
        }
        let directions = corrected.directions.as_ref();
        if self.records.len() != STAR_GPU_RECORD_COUNT
            || self.astrometry.len() != STAR_GPU_RECORD_COUNT
            || directions.len() != STAR_GPU_RECORD_COUNT
            || directions.len() != self.records.len()
        {
            return Err(StarPrepareError::InvalidRecordCount {
                records: self.records.len(),
                directions: directions.len(),
            });
        }
        let mut instances = Box::new([StarInstanceGpu::default(); STAR_GPU_RECORD_COUNT]);
        for (index, ((record, astrometry), direction)) in self
            .records
            .iter()
            .zip(self.astrometry.iter())
            .zip(directions.iter())
            .enumerate()
        {
            if record.hyg_id != astrometry.id || record.hyg_id != direction.id {
                return Err(StarPrepareError::RecordMismatch { index });
            }
            let norm_squared = direction
                .direction_eqj
                .iter()
                .map(|component| component * component)
                .sum::<f64>();
            if !record.apparent_v_mag.is_finite()
                || !record.color_index_bv.is_finite()
                || !astrometry
                    .position_eqj_pc
                    .iter()
                    .all(|component| component.is_finite())
                || !astrometry
                    .velocity_eqj_pc_per_jyear
                    .iter()
                    .all(|component| component.is_finite())
                || !direction
                    .direction_eqj
                    .iter()
                    .all(|component| component.is_finite())
                || !norm_squared.is_finite()
                || (norm_squared - 1.0).abs() > UNIT_DIRECTION_TOLERANCE
            {
                return Err(StarPrepareError::InvalidAstrometry { index });
            }
            let direction_eqj = direction.direction_eqj.map(|component| component as f32);
            let color_linear = color_index_to_linear_rgb(record.color_index_bv);
            if !direction_eqj.iter().all(|component| component.is_finite())
                || !color_linear.iter().all(|component| component.is_finite())
            {
                return Err(StarPrepareError::InvalidAstrometry { index });
            }
            instances[index] = StarInstanceGpu {
                direction_eqj,
                visual_magnitude: record.apparent_v_mag,
                color_linear,
                catalog_star_id: record.hyg_id,
            };
        }
        Ok(instances)
    }
}

/// Version 1 bounded B−V interpolation in sRGB followed by one linearization.
fn color_index_to_linear_rgb(color_index_bv: f32) -> [f32; 3] {
    let bounded = color_index_bv.clamp(BV_COLOR_NODES[0].0, BV_COLOR_NODES[6].0);
    let upper = BV_COLOR_NODES
        .iter()
        .position(|(index, _)| *index >= bounded)
        .unwrap_or(BV_COLOR_NODES.len() - 1);
    let srgb = if upper == 0 {
        BV_COLOR_NODES[0].1
    } else {
        let (low_index, low_rgb) = BV_COLOR_NODES[upper - 1];
        let (high_index, high_rgb) = BV_COLOR_NODES[upper];
        let amount = (bounded - low_index) / (high_index - low_index);
        std::array::from_fn(|channel| {
            low_rgb[channel] + (high_rgb[channel] - low_rgb[channel]) * amount
        })
    };
    srgb.map(srgb_channel_to_linear)
}

fn srgb_channel_to_linear(channel: f32) -> f32 {
    if channel <= 0.04045 {
        channel / 12.92
    } else {
        ((channel + 0.055) / 1.055).powf(2.4)
    }
}

const _: () = {
    assert!(std::mem::size_of::<StarInstanceGpu>() == STAR_GPU_RECORD_BYTES);
    assert!(std::mem::align_of::<StarInstanceGpu>() == 16);
    assert!(std::mem::offset_of!(StarInstanceGpu, direction_eqj) == 0);
    assert!(std::mem::offset_of!(StarInstanceGpu, visual_magnitude) == 12);
    assert!(std::mem::offset_of!(StarInstanceGpu, color_linear) == 16);
    assert!(std::mem::offset_of!(StarInstanceGpu, catalog_star_id) == 28);
};
