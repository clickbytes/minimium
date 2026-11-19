use crate::{
    application::SettingsStore,
    client_commands::{
        format_time_value, property_spec, ClientPropertyCommand, ClientPropertyKey as Key,
        ClientPropertyValue as Value,
    },
    options::ClientOptions,
};
use minimium_client_runtime::ConnectionController;
use minimium_commands::{CommandError, CommandErrorCode};

pub(super) struct TextProperties {
    pub(super) settings: SettingsStore,
}

impl TextProperties {
    pub fn new(options: &ClientOptions) -> Self {
        let mut settings = SettingsStore::new(&options.startup_settings);
        for value in options.startup_settings.iter() {
            let key = value.key();
            if !matches!(key, Key::Locale | Key::InputBindings) {
                settings.set_unavailable(key.key(), "text frontend has no renderer".to_owned());
            }
        }
        Self { settings }
    }

    pub fn session_settings(&self) -> std::collections::BTreeMap<String, String> {
        self.settings.session_values()
    }

    pub fn execute(
        &mut self,
        command: ClientPropertyCommand,
        options: &mut ClientOptions,
        connection: &ConnectionController,
        localizer: &minimium_localization::Localizer,
    ) -> Result<String, CommandError> {
        match command {
            ClientPropertyCommand::Get(key) => {
                match key {
                    Key::WorldTimeMode
                    | Key::WorldTimeScale
                    | Key::WorldTimeOfflinePolicy
                    | Key::WorldTimeUtc
                    | Key::WorldTimeRevision => {
                        let frame = connection
                            .session()
                            .map(|session| session.environment_frame())
                            .unwrap_or_else(crate::environment::EnvironmentFrameInput::unavailable);
                        Ok(format_time_value(key, frame).unwrap_or_else(|| {
                            format!(
                                "{} requested=unavailable effective=unavailable default=n/a source=unavailable reason=time property data unavailable",
                                key.key()
                            )
                        }))
                    }
                    Key::MovementFly | Key::MovementSpeed | Key::MovementAccel => {
                        let policy = connection.movement;
                        let value = match key {
                            Key::MovementFly => connection.flight_enabled.to_string(),
                            Key::MovementSpeed => policy.speed.to_string(),
                            Key::MovementAccel => policy.accel.to_string(),
                            _ => unreachable!("movement property key was matched"),
                        };
                        Ok(format!(
                            "{} requested={} effective={} default=n/a source=server_policy revision={}",
                            key.key(), value, value, policy.revision
                        ))
                    }
                    Key::Locale
                    | Key::InputBindings
                    | Key::CameraFov
                    | Key::MouseSensitivity
                    | Key::SkyQuality
                    | Key::SkyNames
                    | Key::MeteorEnabled
                    | Key::MeteorRate
                    | Key::MeteorQuality
                    | Key::MeteorSeed
                    | Key::MeteorAudio
                    | Key::MeteorVolume
                    | Key::CloudDetail
                    | Key::AtmosphereDebug
                    | Key::MoonFlux
                    | Key::ExposureEv
                    | Key::TerrainDebug
                    | Key::CloudDebug
                    | Key::SkyDemo => Ok(self.get(key, options)),
                }
            }
            ClientPropertyCommand::Set(value) => match value {
                crate::client_commands::ClientPropertyValue::InputBindings(bindings) => {
                    options.input_bindings = bindings.clone();
                    self.settings.set(Value::InputBindings(bindings));
                    Ok(self.get(Key::InputBindings, options))
                }
                other => Err(unsupported_context(other.key().key())),
            },
            ClientPropertyCommand::Reset(key) => {
                if key != Key::InputBindings {
                    return Err(unsupported_context(key.key()));
                }
                match self.settings.reset(key.key()) {
                    Some(Value::InputBindings(bindings)) => options.input_bindings = bindings,
                    _ => unreachable!("local startup setting is validated"),
                }
                Ok(self.get(key, options))
            }
            ClientPropertyCommand::Help(key) => {
                Ok(crate::client_commands::format_property_help_localized(
                    property_spec(key.key()).expect("parsed property key exists"),
                    localizer,
                    options.locale,
                ))
            }
            ClientPropertyCommand::List(prefix) => {
                Ok(crate::client_commands::format_property_list_localized(
                    prefix.as_deref(),
                    localizer,
                    options.locale,
                ))
            }
            ClientPropertyCommand::CommandHelp(name) => Ok(crate::client_commands::command_help(
                &name,
                localizer,
                options.locale,
            )),
            ClientPropertyCommand::CommandHelpGroup(prefix) => Ok(
                crate::client_commands::command_help_group(&prefix, localizer, options.locale),
            ),
        }
    }

    fn get(&self, key: Key, _options: &ClientOptions) -> String {
        let requested = self
            .settings
            .requested(key.key())
            .map(str::to_owned)
            .expect("local setting is present in the startup catalog");
        let source = self.settings.source(key.key());
        let effective = self.settings.effective(key.key()).unwrap_or("unavailable");
        let reason = self.settings.difference_reason(key.key()).unwrap_or("none");
        let default = property_spec(key.key())
            .and_then(|spec| spec.default)
            .unwrap_or("n/a");
        format!(
            "{} requested={} effective={} default={} source={} reason={}",
            key.key(),
            requested,
            effective,
            default,
            source.as_str(),
            reason
        )
    }
}

fn unsupported_context(key: &str) -> CommandError {
    CommandError::new(
        CommandErrorCode::UnsupportedContext,
        Some(key),
        "renderer preference cannot be applied by the text frontend",
        Some("use the desktop renderer or config validate/show"),
    )
}
