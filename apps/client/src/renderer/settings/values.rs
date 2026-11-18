use super::*;
impl DesktopHost {
    pub(super) fn property_line(&self, key: Key, changed: bool) -> String {
        if let Some(line) = crate::client_commands::format_time_value(key, self.environment_frame) {
            return line;
        }
        let default = crate::client_commands::property_spec(key.key())
            .and_then(|spec| spec.default)
            .unwrap_or("n/a")
            .to_owned();
        let (requested, effective, difference_reason) = match key {
            Key::Locale => self.stored_property(key),
            Key::CameraFov => self.stored_property(key),
            Key::MouseSensitivity => self.stored_property(key),
            Key::InputBindings => self.stored_property(key),
            Key::SkyQuality => self.stored_property(key),
            Key::SkyNames => self.stored_property(key),
            Key::MeteorAudio => (
                self.meteors.audio.enabled.to_string(),
                (self.meteors.audio.enabled && self.meteors.audio.backend_status() == "ready")
                    .to_string(),
                Some(self.meteors.audio.backend_status().into()),
            ),
            Key::MeteorEnabled => {
                let (enabled, reason) = self.meteors.enabled_status();
                (
                    self.application
                        .settings
                        .requested(key.key())
                        .expect("local startup setting exists")
                        .into(),
                    enabled.to_string(),
                    reason,
                )
            }
            Key::MeteorRate => self.stored_property(key),
            Key::MeteorSeed => self.stored_property(key),
            Key::MeteorQuality => (
                self.meteors.quality.label().into(),
                self.meteors
                    .effective_quality(self.sky_quality.effective)
                    .label()
                    .into(),
                (self.sky_quality.effective == minimium_renderer::quality::SkyQuality::Low)
                    .then(|| "sky quality limits meteor visibility to low".into()),
            ),
            Key::MeteorVolume => self.stored_property(key),
            Key::CloudDetail => self.stored_property(key),
            Key::AtmosphereDebug => self.stored_property(key),
            Key::MoonFlux => self.stored_property(key),
            Key::ExposureEv => self.stored_property(key),
            Key::TerrainDebug => self.stored_property(key),
            Key::CloudDebug => self.stored_property(key),
            Key::SkyDemo => self.stored_property(key),
            Key::MovementFly => (
                self.application
                    .connection_view()
                    .flight_enabled
                    .to_string(),
                self.application
                    .connection_view()
                    .flight_enabled
                    .to_string(),
                Some("local queued input; server confirmation is unavailable".to_owned()),
            ),
            Key::MovementSpeed => (
                self.application
                    .connection_view()
                    .movement
                    .speed
                    .to_string(),
                self.application
                    .connection_view()
                    .movement
                    .speed
                    .to_string(),
                Some("local queued input; server confirmation is unavailable".to_owned()),
            ),
            Key::MovementAccel => (
                self.application
                    .connection_view()
                    .movement
                    .accel
                    .to_string(),
                self.application
                    .connection_view()
                    .movement
                    .accel
                    .to_string(),
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

    fn stored_property(&self, key: Key) -> (String, String, Option<String>) {
        let store = &self.application.settings;
        (
            store
                .requested(key.key())
                .expect("local startup setting exists")
                .to_owned(),
            store
                .effective(key.key())
                .unwrap_or("unavailable")
                .to_owned(),
            store.difference_reason(key.key()).map(str::to_owned),
        )
    }
}
