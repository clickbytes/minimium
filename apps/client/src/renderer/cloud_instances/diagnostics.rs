use super::CloudInstances;
use crate::environment::weather::clouds::MODEL_REVISION;

#[derive(Default)]
pub(super) struct Diagnostics {
    pub(super) physical_seconds: f64,
    pub(super) clusters: usize,
    pub(super) instances: usize,
    pub(super) aggregates: usize,
    pub(super) merged: usize,
    pub(super) milliseconds: f64,
    pub(super) cache_bytes: usize,
    pub(super) families: [usize; 6],
    pub(super) eye_density: f64,
    pub(super) vertical_optical: f64,
}

impl CloudInstances {
    pub(in crate::renderer) fn status(&self, utc_ms: Option<i64>) -> String {
        let Some(request) = &self.published else {
            return "clouds preparing; effects await a matching field".into();
        };
        let d = &self.diagnostics;
        let field = &self.frame.as_ref().unwrap().fields[0];
        let lag = (utc_ms.unwrap_or(0) as f64 / 1000.0 - request.start_seconds - request.duration)
            .max(0.0);
        format!("cloud model {MODEL_REVISION} field={} clock={:?} | clusters={} ellipsoids={}/{} (retained {}) aggregates={} (retained {}) merged={} | CPU {:.1} ms lag {:.1} world s | GPU buffers/cache textures {} bytes CPU field cache estimate {} bytes{} | families cu/sc/st/ns/ci/cb={:?} endpoint eye density={:.6}/m vertical tau={:.2}",
            field.revision, field.clock_revision, d.clusters, self.draw_count, self.active_limit, d.instances,
            self.far.visible_tiles(self.active_aggregate_limit), d.aggregates, d.merged, d.milliseconds, lag, Self::BYTES, d.cache_bytes,
            if self.failed { " | worker unavailable; retained frame" } else { "" }, d.families, d.eye_density, d.vertical_optical)
    }
}
