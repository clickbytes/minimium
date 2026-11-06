use crate::{
    client_commands::{
        format_time_value, property_spec, ClientPropertyCommand, ClientPropertyKey as Key,
        ClientPropertyValue as Value,
    },
    options::ClientOptions,
};
use minimium_client_runtime::ConnectionController;
use minimium_commands::PropertySource;
use std::collections::BTreeMap;

pub(super) struct TextProperties {
    baseline: ClientOptions,
    session: BTreeMap<String, String>,
    persistent_session: BTreeMap<String, String>,
}

impl TextProperties {
    pub fn new(options: &ClientOptions) -> Self {
        Self {
            baseline: options.clone(),
            session: BTreeMap::new(),
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
    ) -> String {
        match command {
            ClientPropertyCommand::Print(text) => text,
            ClientPropertyCommand::Get(key) => {
                let frame = connection
                    .session()
                    .map(|session| session.environment_frame())
                    .unwrap_or_else(crate::environment::EnvironmentFrameInput::unavailable);
                format_time_value(key, frame).unwrap_or(self.get(key, options))
            }
            ClientPropertyCommand::Set(value) => self.set(value, options),
            ClientPropertyCommand::Reset(key) => self.reset(key, options),
        }
    }

    pub fn get(&self, key: Key, options: &ClientOptions) -> String {
        let requested = current_value(key, options);
        let source = if self.session.contains_key(key.key()) {
            PropertySource::Session
        } else {
            options
                .preference_sources
                .get(key.key())
                .copied()
                .unwrap_or(PropertySource::Default)
        };
        let default = property_spec(key.key())
            .and_then(|spec| spec.default)
            .unwrap_or("n/a");
        format!(
            "{} requested={} effective=unavailable default={} source={} reason=text frontend has no renderer",
            key.key(), requested, default, source.as_str()
        )
    }

    pub fn set(&mut self, value: Value, options: &mut ClientOptions) -> String {
        let (key, text) = value_text(&value);
        store_value(value, options);
        self.session.insert(key.key().to_owned(), text.clone());
        if property_spec(key.key()).is_some_and(|spec| spec.persistent) {
            self.persistent_session.insert(key.key().to_owned(), text);
        }
        self.get(key, options)
    }

    pub fn reset(&mut self, key: Key, options: &mut ClientOptions) -> String {
        restore_value(key, &self.baseline, options);
        self.session.remove(key.key());
        self.persistent_session.remove(key.key());
        self.get(key, options)
    }
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

fn value_text(value: &Value) -> (Key, String) {
    match value {
        Value::CameraFov(value) => (Key::CameraFov, value.to_string()),
        Value::MouseSensitivity(value) => (Key::MouseSensitivity, value.to_string()),
        Value::SkyQuality(value) => (Key::SkyQuality, value.label().to_owned()),
        Value::SkyNames(value) => (Key::SkyNames, value.to_string()),
        Value::CloudDetail(value) => (Key::CloudDetail, value.to_string()),
        Value::AtmosphereDebug(value) => (Key::AtmosphereDebug, atmosphere(*value).to_owned()),
        Value::TerrainDebug(value) => (Key::TerrainDebug, terrain(*value).to_owned()),
        Value::CloudDebug(value) => (Key::CloudDebug, clouds(*value).to_owned()),
        Value::SkyDemo(value) => (Key::SkyDemo, value.to_string()),
        Value::MovementFly(_)
        | Value::MovementSpeed(_)
        | Value::MovementAccel(_)
        | Value::WorldTimeMode(_)
        | Value::WorldTimeScale(_)
        | Value::WorldTimeOfflinePolicy(_) => unreachable!("owned by a runtime property handler"),
    }
}

fn store_value(value: Value, options: &mut ClientOptions) {
    match value {
        Value::CameraFov(value) => options.startup_fov = value,
        Value::MouseSensitivity(value) => options.startup_mouse_sensitivity = value,
        Value::SkyQuality(value) => options.sky_quality = value,
        Value::SkyNames(value) => options.startup_sky_names = value,
        Value::CloudDetail(value) => options.startup_cloud_detail = value,
        Value::AtmosphereDebug(value) => options.startup_atmosphere_debug = value,
        Value::TerrainDebug(value) => options.startup_terrain_debug = value,
        Value::CloudDebug(value) => options.startup_cloud_debug = value,
        Value::SkyDemo(value) => options.startup_sky_demo = value,
        Value::MovementFly(_)
        | Value::MovementSpeed(_)
        | Value::MovementAccel(_)
        | Value::WorldTimeMode(_)
        | Value::WorldTimeScale(_)
        | Value::WorldTimeOfflinePolicy(_) => unreachable!("owned by a runtime property handler"),
    }
}

fn restore_value(key: Key, baseline: &ClientOptions, options: &mut ClientOptions) {
    match key {
        Key::CameraFov => options.startup_fov = baseline.startup_fov,
        Key::MouseSensitivity => {
            options.startup_mouse_sensitivity = baseline.startup_mouse_sensitivity
        }
        Key::SkyQuality => options.sky_quality = baseline.sky_quality,
        Key::SkyNames => options.startup_sky_names = baseline.startup_sky_names,
        Key::CloudDetail => options.startup_cloud_detail = baseline.startup_cloud_detail,
        Key::AtmosphereDebug => {
            options.startup_atmosphere_debug = baseline.startup_atmosphere_debug
        }
        Key::TerrainDebug => options.startup_terrain_debug = baseline.startup_terrain_debug,
        Key::CloudDebug => options.startup_cloud_debug = baseline.startup_cloud_debug,
        Key::SkyDemo => options.startup_sky_demo = baseline.startup_sky_demo,
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
