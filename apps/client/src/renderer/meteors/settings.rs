use super::MeteorRuntime;
use crate::client_commands::ClientPropertyValue as Value;
use minimium_renderer::quality::SkyQuality;

impl MeteorRuntime {
    pub(in crate::renderer) fn set_debug_mode(
        &mut self,
        mode: crate::client_commands::MeteorDebug,
    ) {
        self.debug_mode = mode;
        self.light_mode = if mode == crate::client_commands::MeteorDebug::Occlusion {
            crate::client_commands::MeteorLightDebug::Transmission
        } else {
            crate::client_commands::MeteorLightDebug::On
        };
        self.debug_revision = self.debug_revision.saturating_add(1);
    }

    pub(in crate::renderer) fn set_light_mode(
        &mut self,
        mode: crate::client_commands::MeteorLightDebug,
    ) {
        self.debug_mode = crate::client_commands::MeteorDebug::Off;
        self.light_mode = mode;
        self.debug_revision = self.debug_revision.saturating_add(1);
    }

    pub(in crate::renderer) fn enabled_status(&self) -> (bool, Option<String>) {
        if !self.spawner.enabled {
            return (false, Some("disabled by local preference".into()));
        }
        match self.context_state {
            super::ContextState::Available => (true, None),
            super::ContextState::Pending => (false, Some("waiting for environment".into())),
            super::ContextState::Unavailable { reason } => (false, Some(reason.into())),
        }
    }
    pub(in crate::renderer) fn configured(values: &crate::application::StartupSettings) -> Self {
        let mut runtime = Self::new();
        for value in values.iter() {
            runtime.apply_setting(value);
        }
        runtime
    }

    pub(in crate::renderer) fn apply_setting(&mut self, value: &Value) -> bool {
        match value {
            Value::MeteorEnabled(enabled) => {
                self.spawner.enabled = *enabled;
                self.spawner.suspend();
                if !enabled {
                    self.clear();
                }
            }
            Value::MeteorRate(rate) => {
                self.spawner.rate = *rate;
                self.spawner.suspend();
            }
            Value::MeteorQuality(quality) => {
                self.quality = *quality;
            }
            Value::MeteorSeed(seed) => {
                self.seed_override = *seed;
                self.spawner.seed_override(*seed);
            }
            Value::MeteorAudio(enabled) => {
                self.audio.set_enabled(*enabled);
            }
            Value::MeteorVolume(volume) => {
                self.audio.set_volume(*volume);
            }
            _ => return false,
        }
        true
    }

    pub(in crate::renderer) fn effective_quality(&self, sky: SkyQuality) -> SkyQuality {
        if sky == SkyQuality::Low {
            SkyQuality::Low
        } else {
            self.quality
        }
    }
}
