use super::MeteorRuntime;
use crate::client_commands::{ClientPropertyKey as Key, ClientPropertyValue as Value};
use minimium_renderer::quality::SkyQuality;

impl MeteorRuntime {
    pub(in crate::renderer) fn enabled_status(&self) -> (bool, Option<String>) {
        if !self.spawner.enabled {
            return (false, Some("disabled by local preference".into()));
        }
        match self.context_state {
            super::ContextState::Available { .. } => (true, None),
            super::ContextState::Pending => (false, Some("waiting for environment".into())),
            super::ContextState::Unavailable { reason } => (false, Some(reason.into())),
        }
    }
    pub(in crate::renderer) fn path_lines(&self) -> Vec<String> {
        self.active
            .iter()
            .map(|event| {
                format!(
                    "meteor {} {}: seed {}, planet-centred SI basis {:?}, fragments {:?}",
                    event.id,
                    event.preset.label(),
                    event.seed,
                    event.basis,
                    event.evolution.as_ref().map(|history| history.births)
                )
            })
            .collect()
    }
    pub(in crate::renderer) fn configured(
        values: &std::collections::BTreeMap<String, String>,
    ) -> Self {
        let mut runtime = Self::new();
        for (key, raw) in values {
            if let Ok(value) = crate::client_commands::parse_setting_value(key, raw) {
                runtime.apply_setting(&value);
            }
        }
        runtime
    }

    pub(in crate::renderer) fn apply_setting(&mut self, value: &Value) -> Option<String> {
        Some(match value {
            Value::MeteorEnabled(enabled) => {
                self.spawner.enabled = *enabled;
                self.spawner.suspend();
                if !enabled {
                    self.clear();
                }
                enabled.to_string()
            }
            Value::MeteorRate(rate) => {
                self.spawner.rate = *rate;
                self.spawner.suspend();
                rate.to_string()
            }
            Value::MeteorQuality(quality) => {
                self.quality = *quality;
                quality.label().into()
            }
            Value::MeteorSeed(seed) => {
                self.seed_override = *seed;
                self.spawner.seed_override(*seed);
                seed.map_or_else(|| "none".into(), |s| s.to_string())
            }
            Value::MeteorAudio(enabled) => {
                self.audio.set_enabled(*enabled);
                enabled.to_string()
            }
            Value::MeteorVolume(volume) => {
                self.audio.set_volume(*volume);
                volume.to_string()
            }
            _ => return None,
        })
    }

    pub(in crate::renderer) fn effective_quality(&self, sky: SkyQuality) -> SkyQuality {
        if sky == SkyQuality::Low {
            SkyQuality::Low
        } else {
            self.quality
        }
    }

    pub(in crate::renderer) fn setting_default(key: Key) -> Option<&'static str> {
        match key {
            Key::MeteorEnabled => Some("true"),
            Key::MeteorRate => Some("1"),
            Key::MeteorQuality => Some("standard"),
            Key::MeteorSeed => Some("none"),
            Key::MeteorAudio => Some("false"),
            Key::MeteorVolume => Some("0.25"),
            _ => None,
        }
    }
}
