//! Temporal selection cache; published cloud transforms remain live each frame.
use crate::{
    cloud_uniform::CloudRecord,
    transient_lighting::{CloudLightOccluder, TransientLightUniform, OCCLUDERS_PER_LIGHT},
};

pub struct CloudLightQuery {
    pub ids: [u64; 4],
    pub eye: [f32; 3],
    pub origin: [f64; 3],
    pub revision: u64,
    pub standard: bool,
    pub now: std::time::Instant,
}

#[derive(Default)]
pub struct CloudLightCache {
    ids: [u64; 4],
    indices: [[Option<usize>; OCCLUDERS_PER_LIGHT]; 4],
    refreshed: Option<std::time::Instant>,
    origin: [f64; 3],
    eye: [f32; 3],
    revision: u64,
    record_count: usize,
    pub scans: u64,
    pub reused: u64,
}

impl CloudLightCache {
    pub fn attach(
        &mut self,
        uniform: &mut TransientLightUniform,
        records: &[CloudRecord],
        query: CloudLightQuery,
    ) {
        uniform.clouds.fill(CloudLightOccluder::default());
        for source in &mut uniform.sources {
            source.range_lifetime[2] = 0.0;
        }
        if !query.standard || uniform.count[0] == 0 || records.is_empty() {
            self.refreshed = None;
            return;
        }
        let moved =
            glam::Vec3::from_array(query.eye).distance(glam::Vec3::from_array(self.eye)) > 1000.0;
        let mut previous_ids = self.ids;
        let mut current_ids = query.ids;
        previous_ids.sort_unstable();
        current_ids.sort_unstable();
        let refresh = self
            .refreshed
            .is_none_or(|at| query.now.saturating_duration_since(at).as_secs_f32() >= 0.5)
            || previous_ids != current_ids
            || self.revision != query.revision
            || self.origin != query.origin
            || self.record_count != records.len()
            || moved;
        if refresh {
            uniform.attach_clouds(records, query.eye, true);
            for light in 0..4 {
                for slot in 0..OCCLUDERS_PER_LIGHT {
                    let id = uniform.clouds[light * OCCLUDERS_PER_LIGHT + slot].east_inverse[3];
                    self.indices[light][slot] =
                        (id > 0.0).then_some((id as usize).saturating_sub(1));
                }
            }
            self.ids = query.ids;
            self.refreshed = Some(query.now);
            self.origin = query.origin;
            self.eye = query.eye;
            self.revision = query.revision;
            self.record_count = records.len();
            self.scans = self.scans.saturating_add(1);
        } else {
            self.reused = self.reused.saturating_add(1);
            for light in 0..uniform.count[0] as usize {
                let mut count = 0;
                let cached = self
                    .ids
                    .iter()
                    .position(|id| *id == query.ids[light])
                    .unwrap_or(light);
                for index in self.indices[cached].iter().flatten() {
                    if let Some(record) = records.get(*index) {
                        uniform.clouds[light * OCCLUDERS_PER_LIGHT + count] =
                            crate::transient_clouds::proxy(record, *index);
                        count += 1;
                    }
                }
                uniform.sources[light].range_lifetime[2] = count as f32;
            }
        }
    }
}
