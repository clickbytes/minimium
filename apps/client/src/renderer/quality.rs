use super::Renderer;
use crate::environment::quality::{SkyQuality, SkyQualityState};

impl Renderer {
    pub(super) fn set_sky_quality(&mut self, requested: SkyQuality) -> String {
        let requested_settings = super::shadows::ShadowSettings::for_quality(requested);
        let mut fallback_reason = None;
        if let Err(error) = self.gpu.set_shadow_settings(requested_settings) {
            fallback_reason = Some(error);
            if requested == SkyQuality::Standard {
                let low = super::shadows::ShadowSettings::for_quality(SkyQuality::Low);
                if let Err(low_error) = self.gpu.set_shadow_settings(low) {
                    fallback_reason = Some(low_error);
                }
            }
        }
        let effective = if self.gpu.shadow_gpu.settings.enabled {
            SkyQuality::Standard
        } else {
            SkyQuality::Low
        };
        let previous = self.sky_quality;
        self.sky_quality = SkyQualityState::new(requested, effective, fallback_reason);
        if previous != self.sky_quality {
            self.metrics.frame_stats.reset_environment_epoch();
        }
        self.update_gpu_memory_metric();
        format!(
            "sky quality requested={} effective={} fallback_reason={}",
            self.sky_quality.requested.label(),
            self.sky_quality.effective.label(),
            self.sky_quality.fallback_reason.unwrap_or("none"),
        )
    }
}
