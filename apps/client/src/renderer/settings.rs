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
            ClientPropertyCommand::Print(text) => text,
        }
    }

    pub(super) fn session_settings(&self) -> BTreeMap<String, String> {
        let mut values = BTreeMap::new();
        if let Some(value) = self.fov_override {
            values.insert(Key::CameraFov.key().to_owned(), value.to_string());
        }
        if let Some(value) = self.mouse_sensitivity_override {
            values.insert(Key::MouseSensitivity.key().to_owned(), value.to_string());
        }
        if let Some(value) = self.sky_quality_override {
            values.insert(Key::SkyQuality.key().to_owned(), value.label().to_owned());
        }
        if let Some(value) = self.sky_names_override {
            values.insert(Key::SkyNames.key().to_owned(), value.to_string());
        }
        if let Some(value) = self.cloud_detail_override {
            values.insert(Key::CloudDetail.key().to_owned(), value.to_string());
        }
        values
    }

    fn set_property(&mut self, value: Value) -> Key {
        match value {
            Value::CameraFov(value) => {
                self.fov_override = Some(value);
                self.fov_degrees = value;
                Key::CameraFov
            }
            Value::MouseSensitivity(value) => {
                self.mouse_sensitivity_override = Some(value);
                self.mouse_sensitivity = value;
                Key::MouseSensitivity
            }
            Value::SkyQuality(value) => {
                self.sky_quality_override = Some(value);
                self.set_sky_quality(value);
                Key::SkyQuality
            }
            Value::SkyNames(value) => {
                self.sky_names_override = Some(value);
                self.show_celestial_names = value;
                Key::SkyNames
            }
            Value::CloudDetail(value) => {
                self.cloud_detail_override = Some(value);
                self.weather.set_detail(value);
                Key::CloudDetail
            }
            Value::AtmosphereDebug(value) => {
                self.atmosphere_debug_override = Some(value);
                self.atmosphere_debug = value;
                self.gpu.atmosphere_gpu.debug = value as u32;
                if value != minimium_renderer::atmosphere_debug::AtmosphereDebug::Off {
                    self.terrain_debug = minimium_renderer::terrain_debug::TerrainDebug::Off;
                    self.terrain_debug_override = Some(self.terrain_debug);
                    self.gpu.terrain_gpu.uniform.debug[0] = 0;
                    self.gpu.terrain_gpu.update(&self.gpu.queue);
                }
                Key::AtmosphereDebug
            }
            Value::TerrainDebug(value) => {
                self.terrain_debug_override = Some(value);
                self.terrain_debug = value;
                self.gpu.terrain_gpu.uniform.debug[0] = value as u32;
                self.gpu.terrain_gpu.update(&self.gpu.queue);
                if value != minimium_renderer::terrain_debug::TerrainDebug::Off {
                    self.atmosphere_debug =
                        minimium_renderer::atmosphere_debug::AtmosphereDebug::Off;
                    self.atmosphere_debug_override = Some(self.atmosphere_debug);
                    self.gpu.atmosphere_gpu.debug = 0;
                }
                Key::TerrainDebug
            }
            Value::CloudDebug(value) => {
                self.cloud_debug_override = Some(value);
                self.weather.debug = value;
                Key::CloudDebug
            }
            Value::SkyDemo(value) => {
                self.sky_demo_override = Some(value);
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
        match key {
            Key::CameraFov => {
                self.fov_override = None;
                self.fov_degrees = self.fov_base;
            }
            Key::MouseSensitivity => {
                self.mouse_sensitivity_override = None;
                self.mouse_sensitivity = self.mouse_sensitivity_base;
            }
            Key::SkyQuality => {
                self.sky_quality_override = None;
                self.set_sky_quality(self.sky_quality_base);
            }
            Key::SkyNames => {
                self.sky_names_override = None;
                self.show_celestial_names = self.sky_names_base;
            }
            Key::CloudDetail => {
                self.cloud_detail_override = None;
                self.weather.set_detail(self.cloud_detail_base);
            }
            Key::AtmosphereDebug => {
                self.atmosphere_debug_override = None;
                self.atmosphere_debug = self.atmosphere_debug_base;
                self.gpu.atmosphere_gpu.debug = self.atmosphere_debug as u32;
                if self.atmosphere_debug
                    != minimium_renderer::atmosphere_debug::AtmosphereDebug::Off
                {
                    self.terrain_debug = minimium_renderer::terrain_debug::TerrainDebug::Off;
                    self.terrain_debug_override = Some(self.terrain_debug);
                    self.gpu.terrain_gpu.uniform.debug[0] = 0;
                    self.gpu.terrain_gpu.update(&self.gpu.queue);
                }
            }
            Key::TerrainDebug => {
                self.terrain_debug_override = None;
                self.terrain_debug = self.terrain_debug_base;
                self.gpu.terrain_gpu.uniform.debug[0] = self.terrain_debug as u32;
                self.gpu.terrain_gpu.update(&self.gpu.queue);
                if self.terrain_debug != minimium_renderer::terrain_debug::TerrainDebug::Off {
                    self.atmosphere_debug =
                        minimium_renderer::atmosphere_debug::AtmosphereDebug::Off;
                    self.atmosphere_debug_override = Some(self.atmosphere_debug);
                    self.gpu.atmosphere_gpu.debug = 0;
                }
            }
            Key::CloudDebug => {
                self.cloud_debug_override = None;
                self.weather.debug = self.cloud_debug_base;
            }
            Key::SkyDemo => {
                self.sky_demo_override = None;
                self.demo_universe_enabled = self.sky_demo_base;
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
        let (requested, effective, default, override_active, difference_reason) = match key {
            Key::CameraFov => (
                self.fov_override.unwrap_or(self.fov_base).to_string(),
                self.fov_degrees.to_string(),
                "70".to_owned(),
                self.fov_override.is_some(),
                None,
            ),
            Key::MouseSensitivity => (
                self.mouse_sensitivity_override
                    .unwrap_or(self.mouse_sensitivity_base)
                    .to_string(),
                self.mouse_sensitivity.to_string(),
                "0.0025".to_owned(),
                self.mouse_sensitivity_override.is_some(),
                None,
            ),
            Key::SkyQuality => (
                self.sky_quality_override
                    .unwrap_or(self.sky_quality_base)
                    .label()
                    .to_owned(),
                self.sky_quality.effective.label().to_owned(),
                "standard".to_owned(),
                self.sky_quality_override.is_some(),
                self.sky_quality.fallback_reason.map(str::to_owned),
            ),
            Key::SkyNames => bool_property(
                self.sky_names_override.unwrap_or(self.sky_names_base),
                self.show_celestial_names,
                false,
                self.sky_names_override.is_some(),
            ),
            Key::CloudDetail => (
                self.cloud_detail_override
                    .unwrap_or(self.cloud_detail_base)
                    .to_string(),
                self.cloud_detail_override
                    .unwrap_or(self.cloud_detail_base)
                    .to_string(),
                "6".to_owned(),
                self.cloud_detail_override.is_some(),
                None,
            ),
            Key::AtmosphereDebug => enum_property(
                debug_atmosphere(
                    self.atmosphere_debug_override
                        .unwrap_or(self.atmosphere_debug_base),
                ),
                debug_atmosphere(self.atmosphere_debug),
                "off",
                self.atmosphere_debug_override.is_some(),
            ),
            Key::TerrainDebug => enum_property(
                debug_terrain(
                    self.terrain_debug_override
                        .unwrap_or(self.terrain_debug_base),
                ),
                debug_terrain(self.terrain_debug),
                "off",
                self.terrain_debug_override.is_some(),
            ),
            Key::CloudDebug => enum_property(
                debug_clouds(self.cloud_debug_override.unwrap_or(self.cloud_debug_base)),
                debug_clouds(self.weather.debug),
                "off",
                self.cloud_debug_override.is_some(),
            ),
            Key::SkyDemo => bool_property(
                self.sky_demo_override.unwrap_or(self.sky_demo_base),
                self.demo_universe_enabled,
                false,
                self.sky_demo_override.is_some(),
            ),
            Key::MovementFly => (
                self.connection.movement.fly.to_string(),
                self.connection.movement.fly.to_string(),
                "false".to_owned(),
                false,
                Some("local queued input; server confirmation is unavailable".to_owned()),
            ),
            Key::MovementSpeed => (
                self.connection.movement.speed.to_string(),
                self.connection.movement.speed.to_string(),
                "5".to_owned(),
                false,
                Some("local queued input; server confirmation is unavailable".to_owned()),
            ),
            Key::MovementAccel => (
                self.connection.movement.accel.to_string(),
                self.connection.movement.accel.to_string(),
                "30".to_owned(),
                false,
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
        } else if override_active {
            PropertySource::Session
        } else {
            self.settings
                .preference_sources
                .get(key.key())
                .copied()
                .unwrap_or(PropertySource::Default)
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
}

fn bool_property(
    value: bool,
    effective: bool,
    default: bool,
    override_active: bool,
) -> (String, String, String, bool, Option<String>) {
    (
        value.to_string(),
        effective.to_string(),
        default.to_string(),
        override_active,
        None,
    )
}

fn enum_property(
    value: String,
    effective: String,
    default: &str,
    override_active: bool,
) -> (String, String, String, bool, Option<String>) {
    (value, effective, default.to_owned(), override_active, None)
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
