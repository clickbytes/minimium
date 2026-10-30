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
}
