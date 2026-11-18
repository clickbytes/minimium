use super::{SettingRecord, SettingValue};
use crate::client_commands::{
    local_setting_specs, parse_setting_value, ClientPropertyKey as Key,
    ClientPropertyValue as Value,
};
use minimium_commands::{CommandError, ConfigResolution, PropertySource};
use std::collections::BTreeMap;

/// Immutable validated startup values, shared by every client host and reset.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct StartupSettings {
    pub(super) records: BTreeMap<String, SettingRecord>,
}

macro_rules! getter {
    ($name:ident, $variant:ident, $ty:ty) => {
        pub(crate) fn $name(&self) -> $ty {
            let Value::$variant(value) = self.value(Key::$variant) else {
                unreachable!("catalog key determines the validated value variant")
            };
            value.clone()
        }
    };
}

impl StartupSettings {
    pub(crate) fn from_resolution(resolution: &ConfigResolution) -> Result<Self, CommandError> {
        let records = resolution
            .iter()
            .map(|(key, resolved)| {
                let value = parse_setting_value(key, &resolved.requested)?;
                Ok((
                    value.key().key().to_owned(),
                    SettingRecord::resolved(SettingValue::new(value), resolved.source),
                ))
            })
            .collect::<Result<_, CommandError>>()?;
        Ok(Self { records })
    }

    pub(crate) fn defaults() -> Self {
        let records = local_setting_specs()
            .map(|spec| {
                let value =
                    parse_setting_value(spec.key, spec.default.expect("local startup default"))
                        .expect("catalog defaults satisfy their own validators");
                (
                    spec.key.to_owned(),
                    SettingRecord::resolved(SettingValue::new(value), PropertySource::Default),
                )
            })
            .collect();
        Self { records }
    }

    pub(crate) fn value(&self, key: Key) -> &Value {
        &self
            .records
            .get(key.key())
            .expect("local setting exists in startup catalog")
            .requested
            .typed
    }
    pub(crate) fn iter(&self) -> impl Iterator<Item = &Value> {
        self.records.values().map(|record| &record.requested.typed)
    }

    getter!(locale, Locale, minimium_localization::Locale);
    getter!(input_bindings, InputBindings, String);
    getter!(
        sky_quality,
        SkyQuality,
        minimium_renderer::quality::SkyQuality
    );
    #[cfg(feature = "desktop")]
    getter!(fov, CameraFov, f32);
    #[cfg(feature = "desktop")]
    getter!(mouse_sensitivity, MouseSensitivity, f32);
    #[cfg(feature = "desktop")]
    getter!(moon_flux, MoonFlux, bool);
    #[cfg(feature = "desktop")]
    getter!(exposure_ev, ExposureEv, Option<f32>);
    #[cfg(feature = "desktop")]
    getter!(sky_names, SkyNames, bool);
    #[cfg(feature = "desktop")]
    getter!(cloud_detail, CloudDetail, u8);
    getter!(
        atmosphere_debug,
        AtmosphereDebug,
        minimium_renderer::atmosphere_debug::AtmosphereDebug
    );
    getter!(
        terrain_debug,
        TerrainDebug,
        minimium_renderer::terrain_debug::TerrainDebug
    );
    #[cfg(feature = "desktop")]
    getter!(
        cloud_debug,
        CloudDebug,
        minimium_renderer::cloud_presentation::CloudDebug
    );
    #[cfg(feature = "desktop")]
    getter!(sky_demo, SkyDemo, bool);
}
