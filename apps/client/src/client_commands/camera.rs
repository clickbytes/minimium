use super::{
    ClientCommand, ClientPropertyCommand as Command, ClientPropertyKey as Key,
    ClientPropertyValue as Property, FrontendAction,
};
use crate::application::SettingsStore;
use minimium_commands::operation::{CommandValue as V, FiniteValue};
use minimium_commands::results::{
    OperationContext, OperationFailure, OperationReceipt, OperationResult, Payload, Stage,
};
use minimium_commands::{CommandError, CommandErrorCode as Code};
use std::{collections::BTreeMap, path::Path};

pub(crate) fn validate_fov(value: f32) -> Result<(), &'static str> {
    if value.is_finite() && (30.0..=110.0).contains(&value) {
        Ok(())
    } else {
        Err("field of view must be finite and between 30 and 110 degrees")
    }
}

pub(crate) fn parse_fov_degrees(value: &str) -> Result<f32, &'static str> {
    let degrees = value.parse::<f32>().map_err(|_| "expected a number")?;
    validate_fov(degrees)?;
    Ok(degrees)
}

pub(super) fn look(yaw: f32, pitch: f32) -> Result<ClientCommand, &'static str> {
    if yaw.is_finite() && (-89.0..=89.0).contains(&pitch) {
        Ok(ClientCommand::CameraLook { yaw, pitch })
    } else {
        Err("yaw must be finite and pitch must be between -89 and 89 degrees")
    }
}

pub(crate) struct Camera<'a> {
    pub fov: &'a mut f32,
    pub yaw_rad: &'a mut f32,
    pub pitch_rad: &'a mut f32,
    pub eye_m: Option<[f64; 3]>,
    pub sample_age_ms: u64,
    pub view: &'static str,
}

pub(crate) enum Request<'a> {
    Fov,
    Change(Option<f32>),
    #[cfg(feature = "console-scripting")]
    ConditionalChange {
        value: Option<f32>,
        expected_base: u64,
        expected_revision: u64,
    },
    Status,
    Look(f32, f32),
    Save(&'a Path),
}

impl<'a> Request<'a> {
    pub(crate) fn from_action(action: &'a FrontendAction) -> Option<Self> {
        Some(match action {
            FrontendAction::CameraStatus => Self::Status,
            FrontendAction::CameraLook { yaw, pitch } => Self::Look(*yaw, *pitch),
            FrontendAction::Property(Command::Get(Key::CameraFov)) => Self::Fov,
            FrontendAction::Property(Command::Set(Property::CameraFov(degrees))) => {
                Self::Change(Some(*degrees))
            }
            FrontendAction::Property(Command::Reset(Key::CameraFov)) => Self::Change(None),
            #[cfg(feature = "console-scripting")]
            FrontendAction::CameraCas {
                value,
                expected_base,
                expected_revision,
            } => Self::ConditionalChange {
                value: *value,
                expected_base: *expected_base,
                expected_revision: *expected_revision,
            },
            FrontendAction::ConfigSave(path) => Self::Save(path),
            _ => return None,
        })
    }
}

pub(crate) fn execute(
    identity: OperationContext,
    request: Request<'_>,
    settings: &mut SettingsStore,
    file_settings: &BTreeMap<String, String>,
    camera: Option<Camera<'_>>,
) -> super::output::ClientResult {
    let mut receipt = OperationReceipt::at(Stage::Completed);
    let result = apply(request, settings, file_settings, camera, &mut receipt);
    let outcome = result.map_err(|e| {
        receipt.stage = Stage::Rejected;
        OperationFailure::command("client_preferences", e)
    });
    super::output::native(OperationResult::new(identity, receipt, outcome))
}

fn apply(
    request: Request<'_>,
    settings: &mut SettingsStore,
    file_settings: &BTreeMap<String, String>,
    camera: Option<Camera<'_>>,
    receipt: &mut OperationReceipt,
) -> Result<Payload, CommandError> {
    if let Request::Save(path) = request {
        let saved =
            crate::config::save_preferences(path, file_settings, &settings.session_values())?;
        return Ok(Payload::Value(V::Record(
            [
                ("path".into(), V::Text(saved.path.display().to_string())),
                ("bytes".into(), V::U64(saved.bytes as u64)),
                ("stage".into(), V::Enum("replaced".into())),
            ]
            .into(),
        )));
    }
    let mut snapshot = settings.fov_snapshot();
    receipt.settings_base = Some(snapshot.version.base.get());
    receipt.revision = Some(snapshot.version.revision);
    if let Request::Fov = request {
        receipt.freshness = Some(minimium_commands::results::property::Freshness::Current);
        return Ok(Payload::Property(settings.fov_property()));
    }
    let camera = camera.ok_or_else(|| {
        CommandError::new(
            Code::Unavailable,
            Some("Renderer"),
            "this operation requires a camera",
            None::<String>,
        )
    })?;
    let number = |v: f64| FiniteValue::new(v).map_or(V::Unit, V::Finite);
    match request {
        Request::Change(value) => {
            snapshot.session_override = value;
            let (version, effective) = settings.restore_fov(snapshot, snapshot.version)?;
            settings.confirm_fov(version, effective)?;
            *camera.fov = effective;
            receipt.revision = Some(version.revision);
            receipt.stage = Stage::OwnerCommitted;
            Ok(Payload::Property(settings.fov_property()))
        }
        #[cfg(feature = "console-scripting")]
        Request::ConditionalChange {
            value,
            expected_base,
            expected_revision,
        } => {
            let (version, effective) =
                settings.change_fov_if(value, expected_base, expected_revision)?;
            settings.confirm_fov(version, effective)?;
            *camera.fov = effective;
            receipt.revision = Some(version.revision);
            receipt.stage = Stage::OwnerCommitted;
            Ok(Payload::Property(settings.fov_property()))
        }
        Request::Look(yaw, pitch) => {
            look(yaw, pitch).map_err(|reason| {
                CommandError::new(
                    Code::InvalidValue,
                    Some("client.camera.look"),
                    reason,
                    None::<String>,
                )
            })?;
            *camera.yaw_rad = yaw.to_radians();
            *camera.pitch_rad = pitch.to_radians();
            receipt.stage = Stage::Realized;
            Ok(Payload::Value(V::Unit))
        }
        Request::Status => {
            let eye = camera.eye_m.map_or(V::Unit, |xyz| {
                V::Record(
                    ["x", "y", "z"]
                        .into_iter()
                        .zip(xyz)
                        .map(|(axis, v)| (axis.into(), number(v)))
                        .collect(),
                )
            });
            Ok(Payload::Value(V::Record(
                [
                    ("fov_degrees".into(), number(f64::from(*camera.fov))),
                    (
                        "yaw_degrees".into(),
                        number(f64::from(*camera.yaw_rad).to_degrees()),
                    ),
                    (
                        "pitch_degrees".into(),
                        number(f64::from(*camera.pitch_rad).to_degrees()),
                    ),
                    ("eye_world_m".into(), eye),
                    ("position_source".into(), V::Enum(camera.view.into())),
                    (
                        "presentation_sample_age_ms".into(),
                        V::U64(camera.sample_age_ms),
                    ),
                    ("settings_base".into(), V::U64(snapshot.version.base.get())),
                    ("revision".into(), V::U64(snapshot.version.revision)),
                ]
                .into(),
            )))
        }
        Request::Save(_) | Request::Fov => unreachable!("handled before renderer access"),
    }
}
