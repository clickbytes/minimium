use super::{shadow, Request};
use crate::environment::weather::clouds::CloudFieldSnapshot;
use std::{collections::VecDeque, sync::Arc};

type MomentEntry = ([Arc<CloudFieldSnapshot>; 2], [f32; 4], Arc<shadow::Cache>);
#[derive(Default)]
pub(super) struct BuildCache {
    pub(super) physical_seconds: f64,
    fields: VecDeque<Arc<CloudFieldSnapshot>>,
    moments: VecDeque<MomentEntry>,
}

impl BuildCache {
    pub(super) fn field(&mut self, request: &Request, seconds: f64) -> Arc<CloudFieldSnapshot> {
        if let Some(field) = self.fields.iter().find(|f| {
            f.seed == request.seed
                && f.seconds == seconds
                && f.clock_revision == request.clock_revision
                && f.revision == request.atlas.revision
                && f.blend == request.blend_at(seconds)
        }) {
            return field.clone();
        }
        let started = std::time::Instant::now();
        let field = Arc::new(CloudFieldSnapshot::new(
            request.seed,
            seconds,
            request.clock_revision,
            request.atlas.clone(),
            request.blend_at(seconds),
        ));
        self.fields.push_back(field.clone());
        self.physical_seconds += started.elapsed().as_secs_f64();
        while self.fields.len() > 4 {
            self.fields.pop_front();
        }
        field
    }
    pub(super) fn moments(
        &mut self,
        fields: &[Arc<CloudFieldSnapshot>; 2],
        region: [f32; 4],
    ) -> Arc<shadow::Cache> {
        if let Some((_, _, cache)) = self.moments.iter().find(|(old, old_region, _)| {
            *old_region == region
                && Arc::ptr_eq(&old[0], &fields[0])
                && Arc::ptr_eq(&old[1], &fields[1])
        }) {
            return cache.clone();
        }
        let started = std::time::Instant::now();
        let cache = Arc::new(shadow::build(fields, region));
        self.physical_seconds += started.elapsed().as_secs_f64();
        self.moments
            .push_back((fields.clone(), region, cache.clone()));
        while self.moments.len() > 2 {
            self.moments.pop_front();
        }
        cache
    }
}
