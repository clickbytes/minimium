use super::*;
impl Renderer {
    pub(super) fn property_line(&self, key: Key, changed: bool) -> String {
        if let Some(line) = crate::client_commands::format_time_value(key, self.environment_frame) {
            return line;
        }
        let (requested, effective, default, difference_reason) = match key {
            Key::Locale => self.stored_property(key, "en"),
            Key::CameraFov => self.stored_property(key, "70"),
            Key::MouseSensitivity => self.stored_property(key, "0.0025"),
            Key::InputBindings => self.stored_property(key, ""),
            Key::SkyQuality => self.stored_property(key, "standard"),
            Key::SkyNames => self.stored_property(key, "false"),
            Key::MeteorAudio => (
                self.meteors.audio.enabled.to_string(),
                (self.meteors.audio.enabled && self.meteors.audio.backend_status() == "ready")
                    .to_string(),
                "false".into(),
                Some(self.meteors.audio.backend_status().into()),
            ),
            Key::MeteorEnabled => {
                let (enabled, reason) = self.meteors.enabled_status();
                (
                    self.application
                        .settings
                        .requested(key.key())
                        .unwrap_or("true")
                        .into(),
                    enabled.to_string(),
                    "true".into(),
                    reason,
                )
            }
            Key::MeteorRate => self.stored_property(key, "1"),
            Key::MeteorSeed => self.stored_property(key, "none"),
            Key::MeteorQuality => (
                self.meteors.quality.label().into(),
                self.meteors
                    .effective_quality(self.sky_quality.effective)
                    .label()
                    .into(),
                "standard".into(),
                (self.sky_quality.effective == minimium_renderer::quality::SkyQuality::Low)
                    .then(|| "sky quality limits meteor visibility to low".into()),
            ),
            Key::MeteorVolume => self.stored_property(key, "0.25"),
            Key::CloudDetail => self.stored_property(key, "6"),
            Key::AtmosphereDebug => self.stored_property(key, "off"),
            Key::TerrainDebug => self.stored_property(key, "off"),
            Key::CloudDebug => self.stored_property(key, "off"),
            Key::SkyDemo => self.stored_property(key, "false"),
            Key::MovementFly => (
                self.application.connection.flight_enabled.to_string(),
                self.application.connection.flight_enabled.to_string(),
                "false".to_owned(),
                Some("local queued input; server confirmation is unavailable".to_owned()),
            ),
            Key::MovementSpeed => (
                self.application.connection.movement.speed.to_string(),
                self.application.connection.movement.speed.to_string(),
                "5".to_owned(),
                Some("local queued input; server confirmation is unavailable".to_owned()),
            ),
            Key::MovementAccel => (
                self.application.connection.movement.accel.to_string(),
                self.application.connection.movement.accel.to_string(),
                "30".to_owned(),
                Some("local queued input; server confirmation is unavailable".to_owned()),
            ),
            Key::WorldTimeMode
            | Key::WorldTimeScale
            | Key::WorldTimeOfflinePolicy
            | Key::WorldTimeUtc
            | Key::WorldTimeRevision => unreachable!("world clock value formatted above"),
        };
        let source = if matches!(
            key,
            Key::MovementFly | Key::MovementSpeed | Key::MovementAccel
        ) {
            PropertySource::Runtime
        } else {
            self.application.settings.source(key.key())
        };
        let metadata = PropertyValue {
            requested: Some(requested.clone()),
            effective: Some(effective.clone()),
            default: Some(default.clone()),
            source,
            difference_reason: difference_reason.clone(),
        };
        format!(
            "{} {}requested={} effective={} default={} source={} lifecycle=live reason={}",
            key.key(),
            if changed { "updated; " } else { "" },
            metadata.requested.as_deref().unwrap_or("unavailable"),
            metadata.effective.as_deref().unwrap_or("unavailable"),
            metadata.default.as_deref().unwrap_or("unavailable"),
            metadata.source.as_str(),
            metadata.difference_reason.as_deref().unwrap_or("none"),
        )
    }

    fn stored_property(&self, key: Key, default: &str) -> (String, String, String, Option<String>) {
        let store = &self.application.settings;
        (
            store.requested(key.key()).unwrap_or(default).to_owned(),
            store.effective(key.key()).unwrap_or(default).to_owned(),
            default.to_owned(),
            store.difference_reason(key.key()).map(str::to_owned),
        )
    }
}
