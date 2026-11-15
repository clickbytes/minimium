mod values;
use super::DesktopHost;
use crate::client_commands::{
    ClientPropertyCommand, ClientPropertyKey as Key, ClientPropertyValue as Value,
};
use minimium_commands::{PropertySource, PropertyValue};
use std::collections::BTreeMap;

impl DesktopHost {
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
        let key = value.key();
        self.application.settings.set(value.clone());
        self.apply_property_value(value);
        key
    }

    fn apply_property_value(&mut self, value: Value) {
        if self.meteors.apply_setting(&value) {
            return;
        }
        match value {
            Value::Locale(value) => {
                self.application.settings.locale = value;
                self.application
                    .ui
                    .console
                    .relocalize(&self.localizer, value);
                self.application
                    .ui
                    .console
                    .refresh_suggestions(&self.localizer, value);
            }
            Value::CameraFov(value) => {
                self.fov_degrees = value;
            }
            Value::MouseSensitivity(value) => {
                self.mouse_sensitivity = value;
            }
            Value::InputBindings(value) => {
                self.application.input = crate::input::InputMapper::from_setting(&value)
                    .expect("input bindings are validated before command execution");
            }
            Value::SkyQuality(value) => {
                self.set_sky_quality(value);
                self.application.settings.set_effective(
                    Value::SkyQuality(self.sky_quality.effective),
                    self.sky_quality.fallback_reason.map(str::to_owned),
                );
            }
            Value::SkyNames(value) => {
                self.show_celestial_names = value;
            }
            Value::MeteorEnabled(_)
            | Value::MeteorRate(_)
            | Value::MeteorQuality(_)
            | Value::MeteorSeed(_)
            | Value::MeteorAudio(_)
            | Value::MeteorVolume(_) => unreachable!("handled above"),
            Value::CloudDetail(value) => {
                self.weather.set_detail(value);
            }
            Value::AtmosphereDebug(value) => {
                self.atmosphere_debug = value;
                self.renderer.atmosphere_gpu.debug = value as u32;
                if value != minimium_renderer::atmosphere_debug::AtmosphereDebug::Off {
                    self.terrain_debug = minimium_renderer::terrain_debug::TerrainDebug::Off;
                    self.application
                        .settings
                        .set(Value::TerrainDebug(self.terrain_debug));
                    self.renderer.terrain_gpu.uniform.debug[0] = 0;
                    self.renderer.terrain_gpu.update(&self.renderer.queue);
                }
            }
            Value::TerrainDebug(value) => {
                self.terrain_debug = value;
                self.renderer.terrain_gpu.uniform.debug[0] = value as u32;
                self.renderer.terrain_gpu.update(&self.renderer.queue);
                if value != minimium_renderer::terrain_debug::TerrainDebug::Off {
                    self.atmosphere_debug =
                        minimium_renderer::atmosphere_debug::AtmosphereDebug::Off;
                    self.application
                        .settings
                        .set(Value::AtmosphereDebug(self.atmosphere_debug));
                    self.renderer.atmosphere_gpu.debug = 0;
                }
            }
            Value::CloudDebug(value) => {
                self.weather.debug = value;
            }
            Value::SkyDemo(value) => {
                self.demo_universe_enabled = value;
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
        let value = self
            .application
            .settings
            .reset(key.key())
            .expect("local startup setting is validated");
        self.apply_property_value(value);
    }
}
