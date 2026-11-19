use super::{EnvironmentFrameInput, SynchronizationQuality};
use minimium_commands::operation::CommandValue as V;
use minimium_environment::console::status::{ClockSource, TimeStatus};

impl EnvironmentFrameInput {
    pub fn clock_status(self) -> Option<TimeStatus> {
        Some(TimeStatus {
            sample: self.committed?,
            source: ClockSource::SessionSample,
            quality: self.quality.label(),
            age_ms: self.clock_diagnostics.and_then(|d| d.sample_age_real_ms),
        })
    }

    pub fn clock_property(self, path: &str) -> Option<V> {
        if let Some(status) = self.clock_status() {
            return status.property(path);
        }
        if self.quality != SynchronizationQuality::LocalDemo {
            return None;
        }
        let clock = self.clock_diagnostics?;
        use minimium_environment::time::{ClockMode, OfflinePolicy};
        Some(match path {
            "world.time.mode" => V::Enum(
                match clock.mode? {
                    ClockMode::RealTime => "real_time",
                    ClockMode::TimeScale => "time_scale",
                }
                .into(),
            ),
            "world.time.offline_policy" => V::Enum(
                match clock.offline_policy? {
                    OfflinePolicy::Advance => "advance",
                    OfflinePolicy::Pause => "pause",
                }
                .into(),
            ),
            "world.time.scale" => V::decimal(i128::from(self.rate_ppm?), 6),
            "world.time.revision" => V::U64(self.revision?),
            "world.time.utc" => V::InstantMs(self.estimated_world_utc_ms?),
            _ => return None,
        })
    }

    pub fn clock_status_value(self) -> Option<V> {
        if let Some(status) = self.clock_status() {
            return Some(status.value());
        }
        if self.quality != SynchronizationQuality::LocalDemo {
            return None;
        }
        Some(V::Record(
            [
                ("source".into(), V::Enum("LocalDemo".into())),
                ("quality".into(), V::Enum(self.quality.label().into())),
                ("utc".into(), self.clock_property("world.time.utc")?),
                ("mode".into(), self.clock_property("world.time.mode")?),
                (
                    "offline_policy".into(),
                    self.clock_property("world.time.offline_policy")?,
                ),
                (
                    "revision".into(),
                    self.clock_property("world.time.revision")?,
                ),
                ("requested_ppm".into(), V::U64(self.rate_ppm?)),
                ("effective_ppm".into(), V::U64(self.rate_ppm?)),
                ("achieved_ppm".into(), V::Unit),
                ("sample_age_ms".into(), V::Unit),
                ("calendar_epoch".into(), V::Unit),
                ("tick".into(), V::Unit),
                ("limit".into(), V::Unit),
            ]
            .into(),
        ))
    }
}
