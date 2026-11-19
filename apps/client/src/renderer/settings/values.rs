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
            Key::CameraFov => {
                return self
                    .application
                    .settings
                    .values
                    .fov_property()
                    .format_with(minimium_commands::operation::CommandValue::encode)
            }
            Key::MeteorAudio
            | Key::MeteorEnabled
            | Key::MeteorRate
            | Key::MeteorSeed
            | Key::MeteorQuality
            | Key::MeteorVolume => unreachable!("typed meteor property operation"),
            Key::MouseSensitivity
            | Key::InputBindings
            | Key::SkyQuality
            | Key::SkyNames
            | Key::CloudDetail
            | Key::AtmosphereDebug
            | Key::MoonFlux
            | Key::ExposureEv
            | Key::TerrainDebug
            | Key::CloudDebug
            | Key::SkyDemo => {
                unreachable!("typed local environment operation")
            }
            Key::MovementFly | Key::MovementSpeed | Key::MovementAccel => {
                unreachable!("shared movement operation")
            }
            Key::WorldTimeMode
            | Key::WorldTimeScale
            | Key::WorldTimeOfflinePolicy
            | Key::WorldTimeUtc
            | Key::WorldTimeRevision
            | Key::WorldTimeAchievedScale => unreachable!("world clock value formatted above"),
        };
        let source = self.application.settings.source(key.key());
        let metadata = PropertyValue {
            lifecycle: crate::client_commands::property_spec(key.key())
                .expect("registered property")
                .lifecycle,
            startup: self.application.settings.startup_projection(key.key()),
            session_override: (source == PropertySource::Session).then(|| requested.clone()),
            revision: None,
            freshness: minimium_commands::results::property::Freshness::Current,
            read_source: minimium_commands::results::property::ReadSource::LocalOwner,
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
