#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StarRecord {
    pub hyg_id: u32,
    pub hip_id: u32,
    pub apparent_v_mag: f32,
    pub color_index_bv: f32,
    pub position_eqj_pc: [f64; 3],
    pub velocity_eqj_pc_per_jyear: [f64; 3],
}

#[derive(Clone, Debug, PartialEq)]
pub struct StarCatalogue {
    records: Box<[StarRecord]>,
}

impl StarCatalogue {
    pub(crate) fn new(records: Vec<StarRecord>) -> Self {
        Self {
            records: records.into_boxed_slice(),
        }
    }

    pub fn records(&self) -> &[StarRecord] {
        &self.records
    }
}
