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
    pub(super) fn meteor_property_operation(
        &mut self,
        identity: &minimium_commands::results::OperationContext,
        action: &crate::client_commands::FrontendAction,
    ) -> Option<minimium_commands::results::OperationResult> {
        use minimium_commands::results::{
            OperationFailure, OperationReceipt, OperationResult, Payload, Stage,
        };
        let crate::client_commands::FrontendAction::Property(command) = action else {
            return None;
        };
        let key = match command {
            ClientPropertyCommand::Get(key) => *key,
            ClientPropertyCommand::Set(value) => value.key(),
            ClientPropertyCommand::Reset(key) => *key,
            _ => return None,
        };
        if !crate::client_commands::meteor_adapter::owns(key) {
            return None;
        }
        match command {
            ClientPropertyCommand::Get(_) => Some(OperationResult::new(
                identity.clone(),
                OperationReceipt::at(Stage::Completed),
                Ok(Payload::Property(self.settings.typed_property(
                    key,
                    crate::client_commands::meteor_adapter::encode,
                ))),
            )),
            _ => Some(OperationResult::new(
                identity.clone(),
                OperationReceipt::at(Stage::Rejected),
                Err(OperationFailure::command(
                    "TextProperties",
                    unsupported_context(key.key()),
                )),
            )),
        }
    }

    pub(super) fn local_environment_operation(
        &mut self,
        identity: &minimium_commands::results::OperationContext,
        action: &crate::client_commands::FrontendAction,
        options: &mut ClientOptions,
    ) -> Option<minimium_commands::results::OperationResult> {
        use minimium_commands::results::{
            OperationFailure, OperationReceipt, OperationResult, Payload, Stage,
        };
        let crate::client_commands::FrontendAction::Property(command) = action else {
            return None;
        };
        let key = match command {
            ClientPropertyCommand::Get(key) => *key,
            ClientPropertyCommand::Set(value) => value.key(),
            ClientPropertyCommand::Reset(key) => *key,
            _ => return None,
        };
        if !crate::client_commands::local_environment::owns(key) {
            return None;
        }
        let changed = match command {
            ClientPropertyCommand::Get(_) => false,
            ClientPropertyCommand::Set(Value::InputBindings(bindings)) => {
                options.input_bindings = bindings.clone();
                self.settings.set(Value::InputBindings(bindings.clone()));
                true
            }
            ClientPropertyCommand::Reset(Key::InputBindings) => {
                let Some(Value::InputBindings(bindings)) = self.settings.reset(key.key()) else {
                    unreachable!("validated startup input bindings")
                };
                options.input_bindings = bindings;
                true
            }
            _ => {
                return Some(OperationResult::new(
                    identity.clone(),
                    OperationReceipt::at(Stage::Rejected),
                    Err(OperationFailure::command(
                        "TextProperties",
                        unsupported_context(key.key()),
                    )),
                ));
            }
        };
        let stage = if changed {
            Stage::OwnerCommitted
        } else {
            Stage::Completed
        };
        Some(OperationResult::new(
            identity.clone(),
            OperationReceipt::at(stage),
            Ok(Payload::Property(self.settings.typed_property(
                key,
                crate::client_commands::local_environment::encode,
            ))),
        ))
    }

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

    pub fn execute(
        &mut self,
        command: ClientPropertyCommand,
        _options: &mut ClientOptions,
        connection: &ConnectionController,
    ) -> Result<String, CommandError> {
        match command {
            ClientPropertyCommand::Get(key) => match key {
                Key::WorldTimeMode
                | Key::WorldTimeScale
                | Key::WorldTimeOfflinePolicy
                | Key::WorldTimeUtc
                | Key::WorldTimeRevision
                | Key::WorldTimeAchievedScale => {
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
                    unreachable!("shared movement operation")
                }
                Key::Locale | Key::CameraFov => Ok(self.get(key)),
                Key::MeteorEnabled
                | Key::MeteorRate
                | Key::MeteorQuality
                | Key::MeteorSeed
                | Key::MeteorAudio
                | Key::MeteorVolume => unreachable!("typed meteor property operation"),
                Key::InputBindings
                | Key::MouseSensitivity
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
            },
            ClientPropertyCommand::Set(value) => Err(unsupported_context(value.key().key())),
            ClientPropertyCommand::Reset(key) => Err(unsupported_context(key.key())),
            ClientPropertyCommand::Help(_)
            | ClientPropertyCommand::List(_)
            | ClientPropertyCommand::CommandHelp(_)
            | ClientPropertyCommand::CommandHelpGroup(_) => unreachable!("shared catalog query"),
        }
    }

    fn get(&self, key: Key) -> String {
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
