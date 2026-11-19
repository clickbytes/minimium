mod values;
use super::DesktopHost;
use crate::client_commands::{
    ClientPropertyCommand, ClientPropertyKey as Key, ClientPropertyValue as Value,
};
use minimium_commands::{PropertySource, PropertyValue};

impl DesktopHost {
    pub(in crate::renderer) fn meteor_property_operation(
        &mut self,
        identity: &minimium_commands::results::OperationContext,
        action: &crate::client_commands::FrontendAction,
    ) -> Option<minimium_commands::results::OperationResult> {
        use minimium_commands::operation::CommandValue as V;
        use minimium_commands::results::{OperationReceipt, OperationResult, Payload, Stage};
        let crate::client_commands::FrontendAction::Property(command) = action else {
            return None;
        };
        let (key, changed) = match command {
            ClientPropertyCommand::Get(key)
                if crate::client_commands::meteor_adapter::owns(*key) =>
            {
                (*key, false)
            }
            ClientPropertyCommand::Set(value)
                if crate::client_commands::meteor_adapter::owns(value.key()) =>
            {
                (self.set_property(value.clone()), true)
            }
            ClientPropertyCommand::Reset(key)
                if crate::client_commands::meteor_adapter::owns(*key) =>
            {
                self.reset_property(*key);
                (*key, true)
            }
            _ => return None,
        };
        let mut property = self
            .application
            .settings
            .values
            .typed_property(key, crate::client_commands::meteor_adapter::encode);
        match key {
            Key::MeteorEnabled => {
                let (enabled, reason) = self.meteors.enabled_status();
                property.effective = Some(V::Bool(enabled));
                property.difference_reason = reason;
            }
            Key::MeteorQuality => {
                let effective = self.meteors.effective_quality(self.sky_quality.effective);
                property.effective = Some(V::Enum(effective.label().into()));
                property.difference_reason = (effective != self.meteors.quality)
                    .then(|| "sky quality limits meteor visibility to low".into());
            }
            Key::MeteorAudio | Key::MeteorVolume => {
                let status = self.meteors.audio.backend_status();
                if status != "ready" {
                    property.effective = if key == Key::MeteorAudio && !self.meteors.audio.enabled {
                        Some(V::Bool(false))
                    } else {
                        None
                    };
                    property.difference_reason = Some(status.into());
                }
            }
            _ => {}
        }
        let stage = if changed {
            Stage::OwnerCommitted
        } else {
            Stage::Completed
        };
        Some(OperationResult::new(
            identity.clone(),
            OperationReceipt::at(stage),
            Ok(Payload::Property(property)),
        ))
    }

    pub(in crate::renderer) fn local_environment_operation(
        &mut self,
        identity: &minimium_commands::results::OperationContext,
        action: &crate::client_commands::FrontendAction,
    ) -> Option<minimium_commands::results::OperationResult> {
        use minimium_commands::results::{OperationReceipt, OperationResult, Payload, Stage};
        let crate::client_commands::FrontendAction::Property(command) = action else {
            return None;
        };
        let (key, changed) = match command {
            ClientPropertyCommand::Get(key)
                if crate::client_commands::local_environment::owns(*key) =>
            {
                (*key, false)
            }
            ClientPropertyCommand::Set(value)
                if crate::client_commands::local_environment::owns(value.key()) =>
            {
                (self.set_property(value.clone()), true)
            }
            ClientPropertyCommand::Reset(key)
                if crate::client_commands::local_environment::owns(*key) =>
            {
                self.reset_property(*key);
                (*key, true)
            }
            _ => return None,
        };
        let property = self
            .application
            .settings
            .values
            .typed_property(key, crate::client_commands::local_environment::encode);
        let stage = if changed {
            Stage::OwnerCommitted
        } else {
            Stage::Completed
        };
        Some(OperationResult::new(
            identity.clone(),
            OperationReceipt::at(stage),
            Ok(Payload::Property(property)),
        ))
    }

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
            ClientPropertyCommand::Help(_)
            | ClientPropertyCommand::List(_)
            | ClientPropertyCommand::CommandHelp(_)
            | ClientPropertyCommand::CommandHelpGroup(_) => unreachable!("shared catalog query"),
        }
    }

    fn set_property(&mut self, value: Value) -> Key {
        let key = value.key();
        self.application.settings.set(value.clone());
        self.apply_property_value(value);
        key
    }

    pub(super) fn apply_property_value(&mut self, value: Value) {
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
            Value::CameraFov(_) => {
                unreachable!("camera settings are applied by the versioned camera adapter")
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
            Value::MoonFlux(enabled) => self.moon_flux = enabled,
            Value::ExposureEv(ev) => self.renderer.fixed_exposure_ev = ev,
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
