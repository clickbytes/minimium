use crate::{
    client_commands::{
        format_time_value, property_spec, ClientPropertyCommand, ClientPropertyKey as Key,
    },
    options::ClientOptions,
};
use minimium_client_runtime::ConnectionController;
use minimium_commands::{CommandError, CommandErrorCode, PropertySource};
use std::collections::BTreeMap;

pub(super) struct TextProperties {
    persistent_session: BTreeMap<String, String>,
}

impl TextProperties {
    pub fn new() -> Self {
        Self {
            persistent_session: BTreeMap::new(),
        }
    }

    pub fn session_settings(&self) -> &BTreeMap<String, String> {
        &self.persistent_session
    }

    pub fn execute(
        &mut self,
        command: ClientPropertyCommand,
        options: &mut ClientOptions,
        connection: &ConnectionController,
    ) -> Result<String, CommandError> {
        match command {
            ClientPropertyCommand::Get(key) => {
                let frame = connection
                    .session()
                    .map(|session| session.environment_frame())
                    .unwrap_or_else(crate::environment::EnvironmentFrameInput::unavailable);
                Ok(format_time_value(key, frame).unwrap_or(self.get(key, options)))
            }
            ClientPropertyCommand::Set(value) => Err(unsupported_context(value.key().key())),
            ClientPropertyCommand::Reset(key) => Err(unsupported_context(key.key())),
            ClientPropertyCommand::Print(text) => Ok(text),
        }
    }

    pub fn get(&self, key: Key, options: &ClientOptions) -> String {
        let requested = current_value(key, options);
        let source = options
            .preference_sources
            .get(key.key())
            .copied()
            .unwrap_or(PropertySource::Default);
        let default = property_spec(key.key())
            .and_then(|spec| spec.default)
            .unwrap_or("n/a");
        format!(
            "{} requested={} effective=unavailable default={} source={} reason=text frontend has no renderer",
            key.key(), requested, default, source.as_str()
        )
    }
}

pub(super) fn print_result(result: Result<String, CommandError>) {
    match result {
        Ok(result) => println!("{result}"),
        Err(error) => println!("{}: {error}", error.code.as_str()),
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
        Key::CameraFov => options.startup_fov.to_string(),
        Key::MouseSensitivity => options.startup_mouse_sensitivity.to_string(),
        Key::SkyQuality => options.sky_quality.label().to_owned(),
        Key::SkyNames => options.startup_sky_names.to_string(),
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
        | Key::WorldTimeRevision => unreachable!("owned by a runtime property handler"),
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
