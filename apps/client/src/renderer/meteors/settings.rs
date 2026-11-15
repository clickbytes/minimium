use super::MeteorRuntime;
use crate::client_commands::{ClientPropertyKey as Key, ClientPropertyValue as Value};
use minimium_renderer::quality::SkyQuality;

impl MeteorRuntime {
    pub(in crate::renderer) fn budget_lines(&self) -> Vec<String> {
        use minimium_renderer::meteors::{
            fragmentation::{BODY_SLOTS, SOLID_SLOTS},
            kinematics::Body,
            MAX_METEOR_HISTORY_SAMPLES,
        };
        let age = self.epoch.elapsed().as_secs_f64();
        let mut solids = 0;
        let mut grains = 0;
        for event in self.active.iter().filter(|event| event.basis.is_some()) {
            if let Some(history) = &event.evolution {
                let elapsed = age - event.start_s;
                if elapsed >= history.duration_s() {
                    continue;
                }
                for slot in 0..BODY_SLOTS {
                    if elapsed >= history.births[slot]
                        && history.sample_body(elapsed, slot).gas_power_w > 0.01
                    {
                        if slot < SOLID_SLOTS {
                            solids += 1;
                        } else {
                            grains += 1;
                        }
                    }
                }
            }
        }
        let history_bytes = super::MAX_ACTIVE_METEOR_EVENTS
            * MAX_METEOR_HISTORY_SAMPLES
            * BODY_SLOTS
            * std::mem::size_of::<Body>();
        let metadata_bytes = super::MAX_ACTIVE_METEOR_EVENTS
            * (std::mem::size_of::<super::ActiveEvent>()
                + std::mem::size_of::<super::evolution::Evolution>());
        vec![format!("meteor CPU: prepare {} us (last valid frame), emitting solid fragments {}, grain packets {}, histories {} bytes, pooled metadata/trains {} bytes, frame {} bytes; GPU meteor timestamp n/a (not instrumented)",
            self.prepare_us, solids, grains, history_bytes, metadata_bytes, std::mem::size_of::<minimium_renderer::meteors::MeteorFrame>())]
    }
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
