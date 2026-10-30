use super::Renderer;

impl Renderer {
    pub(super) fn update_gpu_memory_metric(&mut self) {
        self.metrics.frame_stats.set_gpu_bytes(
            self.gpu.fixed_gpu_bytes
                + u64::from(self.gpu.size.width.max(1))
                    * u64::from(self.gpu.size.height.max(1))
                    * 4,
        );
    }

    pub(super) fn record_environment_render_metrics(&mut self, presented: bool) {
        let draws = self.gpu.environment_draw_calls;
        self.metrics.frame_stats.record_environment_frame(
            presented,
            crate::stats::environment::EnvironmentDrawCounts {
                sky: draws.sky,
                stars: draws.stars,
                sun: draws.sun,
                moon: draws.moon,
                halos: draws.halos,
                shadows: draws.shadows,
            },
        );
        if let Some(duration) = self.gpu.last_shadow_prepare_cpu {
            self.metrics.frame_stats.record_shadow_cpu(duration);
        }
        if let Some(duration) = self.gpu.last_environment_upload_cpu {
            self.metrics
                .frame_stats
                .record_environment_upload_cpu(duration);
        }
    }
}
