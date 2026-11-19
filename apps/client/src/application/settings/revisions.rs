use super::{ClientPropertyValue, SettingRecord, SettingsStore};
use crate::client_commands::{camera::validate_fov, ClientPropertyKey};
use minimium_commands::operation::{CommandValue as V, FiniteValue};
use minimium_commands::results::property::{
    ConfigurationSource, Freshness, ReadSource, StartupValue,
};
use minimium_commands::{
    CommandError, CommandErrorCode as Code, PropertyLifecycle, PropertySource, PropertyValue,
};
use std::sync::atomic::{AtomicU64, Ordering};

/// Unique within the application lifetime, including replacement of a settings owner.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct SettingsBaseId(u64);
impl SettingsBaseId {
    pub(super) fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        Self(
            NEXT.try_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                value.checked_add(1)
            })
            .expect("settings generation space exhausted"),
        )
    }
    pub(crate) fn get(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PropertyRevision {
    pub base: SettingsBaseId,
    pub revision: u64,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct FovSnapshot {
    pub version: PropertyRevision,
    pub session_override: Option<f32>,
}
const KEY: &str = ClientPropertyKey::CameraFov.key();

fn error(code: Code, reason: &str) -> CommandError {
    CommandError::new(code, Some(KEY), reason, None::<String>)
}
fn number(fov: f32) -> V {
    V::Finite(FiniteValue::new(f64::from(fov)).expect("validated fov"))
}
fn degrees(r: &SettingRecord) -> f32 {
    let ClientPropertyValue::CameraFov(fov) = r.requested.typed else {
        unreachable!("FOV record")
    };
    fov
}

impl SettingsStore {
    pub(crate) fn fov_snapshot(&self) -> FovSnapshot {
        let r = &self.records[KEY];
        FovSnapshot {
            version: self.fov_version,
            session_override: (r.source == PropertySource::Session).then(|| degrees(r)),
        }
    }
    fn check_fov(&self, expected: PropertyRevision) -> Result<(), CommandError> {
        if expected.base != self.fov_version.base {
            return Err(error(
                Code::SettingsBaseChanged,
                "settings owner or startup configuration changed",
            ));
        }
        if expected.revision != self.fov_version.revision {
            return Err(error(
                Code::RevisionConflict,
                "FOV changed since the snapshot",
            ));
        }
        Ok(())
    }
    /// Restore the captured override against a caller-selected current revision of the same base.
    pub(crate) fn restore_fov(
        &mut self,
        snapshot: FovSnapshot,
        expected: PropertyRevision,
    ) -> Result<(PropertyRevision, f32), CommandError> {
        self.check_fov(expected)?;
        if snapshot.version.base != expected.base {
            return Err(error(
                Code::SettingsBaseChanged,
                "snapshot belongs to a different startup configuration",
            ));
        }
        let revision = expected
            .revision
            .checked_add(1)
            .ok_or_else(|| error(Code::RevisionConflict, "FOV revision exhausted"))?;
        let fov = match snapshot.session_override {
            Some(fov) => {
                validate_fov(fov).map_err(|reason| error(Code::InvalidValue, reason))?;
                self.set(ClientPropertyValue::CameraFov(fov));
                fov
            }
            None => {
                let startup = self.startup[KEY].clone();
                let fov = degrees(&startup);
                self.records.insert(KEY.into(), startup);
                fov
            }
        };
        self.set_unavailable(
            KEY,
            "waiting for the camera to apply the requested FOV".into(),
        );
        self.fov_version.revision = revision;
        Ok((self.fov_version, fov))
    }
    /// Compare the current owner revision and base, then apply one requested
    /// override state. A missing override resets to this owner's startup layer.
    #[cfg(feature = "console-scripting")]
    pub(crate) fn change_fov_if(
        &mut self,
        session_override: Option<f32>,
        expected_base: u64,
        expected_revision: u64,
    ) -> Result<(PropertyRevision, f32), CommandError> {
        let expected = PropertyRevision {
            base: SettingsBaseId(expected_base),
            revision: expected_revision,
        };
        self.restore_fov(
            FovSnapshot {
                version: expected,
                session_override,
            },
            expected,
        )
    }
    pub(crate) fn confirm_fov(
        &mut self,
        expected: PropertyRevision,
        effective: f32,
    ) -> Result<(), CommandError> {
        self.check_fov(expected)?;
        validate_fov(effective).map_err(|reason| error(Code::InvalidValue, reason))?;
        let r = self.records.get_mut(KEY).expect("resolved FOV preference");
        r.effective = Some(super::SettingValue::new(ClientPropertyValue::CameraFov(
            effective,
        )));
        r.difference_reason =
            (effective != degrees(r)).then(|| "renderer applied a different FOV".into());
        Ok(())
    }
    pub(crate) fn fov_property(&self) -> PropertyValue<V> {
        let startup = &self.startup[KEY];
        let r = &self.records[KEY];
        let effective = r.effective.as_ref().map(|v| {
            let ClientPropertyValue::CameraFov(fov) = v.typed else {
                unreachable!("FOV record")
            };
            number(fov)
        });
        PropertyValue {
            lifecycle: PropertyLifecycle::Live,
            startup: Some(StartupValue {
                value: number(degrees(startup)),
                source: ConfigurationSource::from_source(startup.source).expect("startup source"),
            }),
            session_override: self.fov_snapshot().session_override.map(number),
            read_source: ReadSource::LocalOwner,
            revision: Some(self.fov_version.revision),
            freshness: Freshness::Current,
            requested: Some(number(degrees(r))),
            effective,
            default: crate::client_commands::property_spec(KEY)
                .and_then(|s| s.default)
                .and_then(|s| s.parse().ok())
                .map(number),
            source: r.source,
            difference_reason: r.difference_reason.clone(),
        }
    }
}
