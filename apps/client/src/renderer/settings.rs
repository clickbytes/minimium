use super::Renderer;
use crate::client_commands::{
    ClientPropertyCommand, ClientPropertyKey as Key, ClientPropertyValue as Value,
};
use minimium_commands::{PropertySource, PropertyValue};
use std::collections::BTreeMap;

impl Renderer {
    pub(super) fn apply_property(&mut self, command: ClientPropertyCommand) -> String {
        match command {
            ClientPropertyCommand::Get(key) => self.property_line(key, false),
            ClientPropertyCommand::Set(value) => {
                let key = self.set_property(value);
                self.property_line(key, true)
            }
            ClientPropertyCommand::Reset(key) => {
                self.reset_property(key);
                self.property_line(key, true)
            }
            ClientPropertyCommand::Help(key) => {
                crate::client_commands::format_property_help_localized(
                    crate::client_commands::property_spec(key.key())
                        .expect("parsed property key exists"),
                    &self.localizer,
                    self.application.settings.locale,
                )
            }
            ClientPropertyCommand::List(prefix) => {
                crate::client_commands::format_property_list_localized(
                    prefix.as_deref(),
                    &self.localizer,
                    self.application.settings.locale,
                )
            }
            ClientPropertyCommand::CommandHelp(name) => crate::client_commands::command_help(
                &name,
                &self.localizer,
                self.application.settings.locale,
            ),
            ClientPropertyCommand::CommandHelpGroup(prefix) => {
                crate::client_commands::command_help_group(
                    &prefix,
                    &self.localizer,
                    self.application.settings.locale,
                )
            }
        }
    }

    pub(super) fn session_settings(&self) -> BTreeMap<String, String> {
        self.application.settings.session_values()
    }

    fn set_property(&mut self, value: Value) -> Key {
        match value {
            Value::Locale(value) => {
                self.application.settings.locale = value;
                self.application.settings.set(
                    Key::Locale.key(),
                    match value {
                        minimium_localization::Locale::English => "en",
                        minimium_localization::Locale::Polish => "pl",
                    }
                    .to_owned(),
                );
                self.application
                    .ui
                    .console
                    .relocalize(&self.localizer, value);
                self.application
                    .ui
                    .console
                    .refresh_suggestions(&self.localizer, value);
                Key::Locale
            }
            Value::CameraFov(value) => {
                self.application
                    .settings
                    .set(Key::CameraFov.key(), value.to_string());
                self.fov_degrees = value;
                Key::CameraFov
            }
            Value::MouseSensitivity(value) => {
                self.application
                    .settings
                    .set(Key::MouseSensitivity.key(), value.to_string());
                self.mouse_sensitivity = value;
                Key::MouseSensitivity
            }
            Value::InputBindings(value) => {
                self.application.input = crate::input::InputMapper::from_setting(&value)
                    .expect("input bindings are validated before command execution");
                self.application
                    .settings
                    .set(Key::InputBindings.key(), value);
                Key::InputBindings
            }
            Value::SkyQuality(value) => {
                self.application
                    .settings
                    .set(Key::SkyQuality.key(), value.label().to_owned());
                self.set_sky_quality(value);
                self.application.settings.set_effective(
                    Key::SkyQuality.key(),
                    self.sky_quality.effective.label().to_owned(),
                    self.sky_quality.fallback_reason.map(str::to_owned),
                );
                Key::SkyQuality
            }
            Value::SkyNames(value) => {
                self.application
                    .settings
                    .set(Key::SkyNames.key(), value.to_string());
                self.show_celestial_names = value;
                Key::SkyNames
            }
            Value::CloudDetail(value) => {
                self.application
                    .settings
                    .set(Key::CloudDetail.key(), value.to_string());
                self.weather.set_detail(value);
                Key::CloudDetail
            }
            Value::AtmosphereDebug(value) => {
                self.application
                    .settings
                    .set(Key::AtmosphereDebug.key(), debug_atmosphere(value));
                self.atmosphere_debug = value;
                self.gpu.atmosphere_gpu.debug = value as u32;
                if value != minimium_renderer::atmosphere_debug::AtmosphereDebug::Off {
                    self.terrain_debug = minimium_renderer::terrain_debug::TerrainDebug::Off;
                    self.application
                        .settings
                        .set(Key::TerrainDebug.key(), "off".to_owned());
                    self.gpu.terrain_gpu.uniform.debug[0] = 0;
                    self.gpu.terrain_gpu.update(&self.gpu.queue);
                }
                Key::AtmosphereDebug
            }
            Value::TerrainDebug(value) => {
                self.application
                    .settings
                    .set(Key::TerrainDebug.key(), debug_terrain(value));
                self.terrain_debug = value;
                self.gpu.terrain_gpu.uniform.debug[0] = value as u32;
                self.gpu.terrain_gpu.update(&self.gpu.queue);
                if value != minimium_renderer::terrain_debug::TerrainDebug::Off {
                    self.atmosphere_debug =
                        minimium_renderer::atmosphere_debug::AtmosphereDebug::Off;
                    self.application
                        .settings
                        .set(Key::AtmosphereDebug.key(), "off".to_owned());
                    self.gpu.atmosphere_gpu.debug = 0;
                }
                Key::TerrainDebug
            }
            Value::CloudDebug(value) => {
                self.application
                    .settings
                    .set(Key::CloudDebug.key(), debug_clouds(value));
                self.weather.debug = value;
                Key::CloudDebug
            }
            Value::SkyDemo(value) => {
                self.application
                    .settings
                    .set(Key::SkyDemo.key(), value.to_string());
                self.demo_universe_enabled = value;
                Key::SkyDemo
            }
            Value::MovementFly(_) | Value::MovementSpeed(_) | Value::MovementAccel(_) => {
                unreachable!("movement properties are handled by the movement owner")
            }
            Value::WorldTimeMode(_)
            | Value::WorldTimeScale(_)
            | Value::WorldTimeOfflinePolicy(_) => {
                unreachable!("world clock mutations use the owner admin channel")
            }
        }
    }

    fn reset_property(&mut self, key: Key) {
        self.application.settings.reset(key.key());
        match key {
            Key::Locale => {
                self.application.settings.locale = minimium_localization::Locale::from_setting(
                    self.application
                        .settings
                        .requested(Key::Locale.key())
                        .unwrap_or("en"),
                )
                .expect("validated startup locale");
                self.application
                    .ui
                    .console
                    .relocalize(&self.localizer, self.application.settings.locale);
                self.application
                    .ui
                    .console
                    .refresh_suggestions(&self.localizer, self.application.settings.locale);
            }
            Key::CameraFov => {
                self.fov_degrees = self
                    .application
                    .settings
                    .requested(Key::CameraFov.key())
                    .unwrap_or("70")
                    .parse()
                    .expect("validated startup FOV");
            }
            Key::MouseSensitivity => {
                self.mouse_sensitivity = self
                    .application
                    .settings
                    .requested(Key::MouseSensitivity.key())
                    .unwrap_or("0.0025")
                    .parse()
                    .expect("validated startup sensitivity");
            }
            Key::InputBindings => {
                let bindings = self
                    .application
                    .settings
                    .requested(Key::InputBindings.key())
                    .unwrap_or("");
                self.application.input = crate::input::InputMapper::from_setting(bindings)
                    .expect("validated startup input bindings");
            }
            Key::SkyQuality => {
                let Value::SkyQuality(value) = crate::client_commands::parse_setting_value(
                    Key::SkyQuality.key(),
                    self.application
                        .settings
                        .requested(Key::SkyQuality.key())
                        .unwrap_or("standard"),
                )
                .expect("validated startup quality") else {
                    unreachable!()
                };
                self.set_sky_quality(value);
                self.application.settings.set_effective(
                    Key::SkyQuality.key(),
                    self.sky_quality.effective.label().to_owned(),
                    self.sky_quality.fallback_reason.map(str::to_owned),
                );
            }
            Key::SkyNames => {
                self.show_celestial_names = self
                    .application
                    .settings
                    .requested(Key::SkyNames.key())
                    .unwrap_or("false")
                    == "true";
            }
            Key::CloudDetail => {
                let detail = self
                    .application
                    .settings
                    .requested(Key::CloudDetail.key())
                    .unwrap_or("6")
                    .parse()
                    .expect("validated startup cloud detail");
                self.weather.set_detail(detail);
            }
            Key::AtmosphereDebug => {
                let Value::AtmosphereDebug(value) = crate::client_commands::parse_setting_value(
                    Key::AtmosphereDebug.key(),
                    self.application
                        .settings
                        .requested(Key::AtmosphereDebug.key())
                        .unwrap_or("off"),
                )
                .expect("validated startup atmosphere debug") else {
                    unreachable!()
                };
                self.atmosphere_debug = value;
                self.gpu.atmosphere_gpu.debug = self.atmosphere_debug as u32;
                if self.atmosphere_debug
                    != minimium_renderer::atmosphere_debug::AtmosphereDebug::Off
                {
                    self.terrain_debug = minimium_renderer::terrain_debug::TerrainDebug::Off;
                    self.application
                        .settings
                        .set(Key::TerrainDebug.key(), "off".to_owned());
                    self.gpu.terrain_gpu.uniform.debug[0] = 0;
                    self.gpu.terrain_gpu.update(&self.gpu.queue);
                }
            }
            Key::TerrainDebug => {
                let Value::TerrainDebug(value) = crate::client_commands::parse_setting_value(
                    Key::TerrainDebug.key(),
                    self.application
                        .settings
                        .requested(Key::TerrainDebug.key())
                        .unwrap_or("off"),
                )
                .expect("validated startup terrain debug") else {
                    unreachable!()
                };
                self.terrain_debug = value;
                self.gpu.terrain_gpu.uniform.debug[0] = self.terrain_debug as u32;
                self.gpu.terrain_gpu.update(&self.gpu.queue);
                if self.terrain_debug != minimium_renderer::terrain_debug::TerrainDebug::Off {
                    self.atmosphere_debug =
                        minimium_renderer::atmosphere_debug::AtmosphereDebug::Off;
                    self.application
                        .settings
                        .set(Key::AtmosphereDebug.key(), "off".to_owned());
                    self.gpu.atmosphere_gpu.debug = 0;
                }
            }
            Key::CloudDebug => {
                let Value::CloudDebug(value) = crate::client_commands::parse_setting_value(
                    Key::CloudDebug.key(),
                    self.application
                        .settings
                        .requested(Key::CloudDebug.key())
                        .unwrap_or("off"),
                )
                .expect("validated startup cloud debug") else {
                    unreachable!()
                };
                self.weather.debug = value;
            }
            Key::SkyDemo => {
                self.demo_universe_enabled = self
                    .application
                    .settings
                    .requested(Key::SkyDemo.key())
                    .unwrap_or("false")
                    == "true";
            }
            Key::MovementFly | Key::MovementSpeed | Key::MovementAccel => {
                unreachable!("movement properties are handled by the movement owner")
            }
            Key::WorldTimeMode
            | Key::WorldTimeScale
            | Key::WorldTimeOfflinePolicy
            | Key::WorldTimeUtc
            | Key::WorldTimeRevision => {
                unreachable!("world clock properties are not renderer settings")
            }
        }
    }

    fn property_line(&self, key: Key, changed: bool) -> String {
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

fn debug_atmosphere(value: minimium_renderer::atmosphere_debug::AtmosphereDebug) -> String {
    match value {
        minimium_renderer::atmosphere_debug::AtmosphereDebug::Off => "off",
        minimium_renderer::atmosphere_debug::AtmosphereDebug::Transmission => "transmission",
        minimium_renderer::atmosphere_debug::AtmosphereDebug::Scattering => "scattering",
        minimium_renderer::atmosphere_debug::AtmosphereDebug::OpticalDepth => "optical_depth",
        minimium_renderer::atmosphere_debug::AtmosphereDebug::Distance => "distance",
        minimium_renderer::atmosphere_debug::AtmosphereDebug::Fog => "fog",
    }
    .to_owned()
}

fn debug_terrain(value: minimium_renderer::terrain_debug::TerrainDebug) -> String {
    match value {
        minimium_renderer::terrain_debug::TerrainDebug::Off => "off",
        minimium_renderer::terrain_debug::TerrainDebug::Albedo => "albedo",
        minimium_renderer::terrain_debug::TerrainDebug::Normals => "normals",
        minimium_renderer::terrain_debug::TerrainDebug::Depth => "depth",
        minimium_renderer::terrain_debug::TerrainDebug::Shadow => "shadow",
        minimium_renderer::terrain_debug::TerrainDebug::BaseColor => "base_color",
        minimium_renderer::terrain_debug::TerrainDebug::Coverage => "coverage",
        minimium_renderer::terrain_debug::TerrainDebug::Habitat => "habitat",
        minimium_renderer::terrain_debug::TerrainDebug::Source => "source",
    }
    .to_owned()
}

fn debug_clouds(value: minimium_environment::weather::CloudDebug) -> String {
    match value {
        minimium_environment::weather::CloudDebug::Off => "off",
        minimium_environment::weather::CloudDebug::Regions => "regions",
        minimium_environment::weather::CloudDebug::Layers => "layers",
        minimium_environment::weather::CloudDebug::Wind => "wind",
        minimium_environment::weather::CloudDebug::Kinds => "kinds",
        minimium_environment::weather::CloudDebug::Detail => "detail",
    }
    .to_owned()
}
