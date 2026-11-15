use crate::{
    application::SettingsStore,
    client_commands::{
        format_time_value, property_spec, ClientPropertyCommand, ClientPropertyKey as Key,
    },
    options::ClientOptions,
};
use minimium_client_runtime::ConnectionController;
use minimium_commands::{CommandError, CommandErrorCode};

pub(super) struct TextProperties {
    settings: SettingsStore,
}

impl TextProperties {
    pub fn new(options: &ClientOptions) -> Self {
        let startup = [
            Key::Locale,
            Key::CameraFov,
            Key::MouseSensitivity,
            Key::InputBindings,
            Key::SkyQuality,
            Key::SkyNames,
            Key::MeteorEnabled,
            Key::MeteorRate,
            Key::MeteorQuality,
            Key::MeteorSeed,
            Key::MeteorAudio,
            Key::MeteorVolume,
            Key::CloudDetail,
            Key::AtmosphereDebug,
            Key::TerrainDebug,
            Key::CloudDebug,
            Key::SkyDemo,
        ]
        .into_iter()
        .map(|key| (key.key().to_owned(), current_value(key, options)))
        .collect();
        let mut settings = SettingsStore::new(startup, options.preference_sources.clone());
        for key in [
            Key::CameraFov,
            Key::MouseSensitivity,
            Key::SkyQuality,
            Key::SkyNames,
            Key::MeteorEnabled,
            Key::MeteorRate,
            Key::MeteorQuality,
            Key::MeteorSeed,
            Key::MeteorAudio,
            Key::MeteorVolume,
            Key::CloudDetail,
            Key::AtmosphereDebug,
            Key::TerrainDebug,
            Key::CloudDebug,
            Key::SkyDemo,
        ] {
            settings.set_unavailable(key.key(), "text frontend has no renderer".to_owned());
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
                    | Key::TerrainDebug
                    | Key::CloudDebug
                    | Key::SkyDemo => Ok(self.get(key, options)),
                }
            }
            ClientPropertyCommand::Set(value) => match value {
                crate::client_commands::ClientPropertyValue::Locale(locale) => {
                    options.locale = locale;
                    self.settings
                        .set(Key::Locale.key(), current_locale(locale).to_owned());
                    Ok(self.get(Key::Locale, options))
                }
                crate::client_commands::ClientPropertyValue::InputBindings(bindings) => {
                    options.input_bindings = bindings.clone();
                    self.settings.set(Key::InputBindings.key(), bindings);
                    Ok(self.get(Key::InputBindings, options))
                }
                other => Err(unsupported_context(other.key().key())),
            },
            ClientPropertyCommand::Reset(key) => {
                if key == Key::Locale {
                    options.locale = options.startup_locale;
                    self.settings.reset(Key::Locale.key());
                    Ok(self.get(Key::Locale, options))
                } else if key == Key::InputBindings {
                    options.input_bindings = options.startup_input_bindings.clone();
                    self.settings.reset(Key::InputBindings.key());
                    Ok(self.get(Key::InputBindings, options))
                } else {
                    Err(unsupported_context(key.key()))
                }
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

    fn get(&self, key: Key, options: &ClientOptions) -> String {
        let requested = self
            .settings
            .requested(key.key())
            .map(str::to_owned)
            .unwrap_or_else(|| current_value(key, options));
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

pub(super) fn print_result(
    result: Result<String, CommandError>,
    localizer: &minimium_localization::Localizer,
    locale: minimium_localization::Locale,
) {
    match result {
        Ok(result) => println!("{result}"),
        Err(error) => println!(
            "{}",
            crate::client_commands::command_error(&error, localizer, locale)
        ),
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

fn current_value(key: Key, options: &ClientOptions) -> String {
    match key {
        Key::Locale => current_locale(options.locale).to_owned(),
        Key::CameraFov => options.startup_fov.to_string(),
        Key::MouseSensitivity => options.startup_mouse_sensitivity.to_string(),
        Key::InputBindings => options.input_bindings.clone(),
        Key::SkyQuality => options.sky_quality.label().to_owned(),
        Key::SkyNames => options.startup_sky_names.to_string(),
        Key::MeteorEnabled
        | Key::MeteorRate
        | Key::MeteorQuality
        | Key::MeteorSeed
        | Key::MeteorAudio => options
            .meteor_preferences
            .get(key.key())
            .cloned()
            .unwrap_or_else(|| {
                match key {
                    Key::MeteorEnabled => "true",
                    Key::MeteorRate => "1",
                    Key::MeteorQuality => "standard",
                    Key::MeteorSeed => "none",
                    _ => "false",
                }
                .into()
            }),
        Key::MeteorVolume => "0.25".into(),
        Key::CloudDetail => options.startup_cloud_detail.to_string(),
        Key::AtmosphereDebug => atmosphere(options.startup_atmosphere_debug).to_owned(),
        Key::TerrainDebug => terrain(options.startup_terrain_debug).to_owned(),
        Key::CloudDebug => clouds(options.startup_cloud_debug).to_owned(),
        Key::SkyDemo => options.startup_sky_demo.to_string(),
        Key::MovementFly
        | Key::MovementSpeed
        | Key::MovementAccel
        | Key::WorldTimeMode
        | Key::WorldTimeScale
        | Key::WorldTimeOfflinePolicy
        | Key::WorldTimeUtc
        | Key::WorldTimeRevision => "unavailable".to_owned(),
    }
}

fn current_locale(locale: minimium_localization::Locale) -> &'static str {
    match locale {
        minimium_localization::Locale::English => "en",
        minimium_localization::Locale::Polish => "pl",
    }
}

fn atmosphere(value: minimium_renderer::atmosphere_debug::AtmosphereDebug) -> &'static str {
    use minimium_renderer::atmosphere_debug::AtmosphereDebug as D;
    match value {
        D::Off => "off",
        D::Transmission => "transmission",
        D::Scattering => "scattering",
        D::OpticalDepth => "optical_depth",
        D::Distance => "distance",
        D::Fog => "fog",
    }
}

fn terrain(value: minimium_renderer::terrain_debug::TerrainDebug) -> &'static str {
    use minimium_renderer::terrain_debug::TerrainDebug as D;
    match value {
        D::Off => "off",
        D::Albedo => "albedo",
        D::Normals => "normals",
        D::Depth => "depth",
        D::Shadow => "shadow",
        D::BaseColor => "base_color",
        D::Coverage => "coverage",
        D::Habitat => "habitat",
        D::Source => "source",
    }
}

fn clouds(value: minimium_environment::weather::CloudDebug) -> &'static str {
    use minimium_environment::weather::CloudDebug as D;
    match value {
        D::Off => "off",
        D::Regions => "regions",
        D::Layers => "layers",
        D::Wind => "wind",
        D::Kinds => "kinds",
        D::Detail => "detail",
    }
}
