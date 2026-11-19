//! Lower registered developer operations from the one catalog binder result.
use super::presentation::{BolidPath, DeveloperOperation as Dev, PresentationOperation};
use minimium_commands::operation::CommandValue as V;
use minimium_commands::syntax::bind::PreparedOperation;
use minimium_commands::{CommandError, CommandErrorCode};

pub(super) fn lower(op: &PreparedOperation) -> Result<PresentationOperation, CommandError> {
    let invalid = || {
        CommandError::new(
            CommandErrorCode::InvalidArguments,
            Some(&op.path),
            "invalid developer presentation arguments",
            Some(&op.spelling),
        )
        .at(op.span)
    };
    let integer = |index: usize| match op.arguments.get(index).map(|a| &a.value) {
        Some(V::U64(value)) => Some(*value),
        _ => None,
    };
    let finite = |index: usize| match op.arguments.get(index).map(|a| &a.value) {
        Some(V::Finite(value)) => Some(value.get() as f32),
        _ => None,
    };
    let developer = match op.path.as_str() {
        "client.weather.clouds.targets" => Dev::Clouds,
        "client.render.bolids.fragments" => Dev::BolidFragments,
        "client.render.bolids.lighting" => Dev::BolidLighting,
        "client.render.bolids.status" => Dev::BolidStatus,
        "client.render.bolids.restore_path" => {
            let path = BolidPath {
                id: integer(0).ok_or_else(invalid)?,
                kind: u8::try_from(integer(1).ok_or_else(invalid)?).map_err(|_| invalid())?,
                seed: integer(2).ok_or_else(invalid)?,
                duration_ms: u16::try_from(integer(3).ok_or_else(invalid)?)
                    .map_err(|_| invalid())?,
                start_world_m: [
                    finite(4).ok_or_else(invalid)?,
                    finite(5).ok_or_else(invalid)?,
                    finite(6).ok_or_else(invalid)?,
                ],
                end_world_m: [
                    finite(7).ok_or_else(invalid)?,
                    finite(8).ok_or_else(invalid)?,
                    finite(9).ok_or_else(invalid)?,
                ],
            };
            let event = minimium_protocol::BolidEvent {
                id: path.id,
                start_simulation_ms: 0,
                seed: path.seed,
                start_world_m: path.start_world_m,
                end_world_m: path.end_world_m,
                duration_ms: path.duration_ms,
                kind: path.kind,
            };
            if !event.valid() {
                return Err(invalid());
            }
            Dev::BolidPath(path)
        }
        "client.render.bolids.replay" | "client.render.bolids.freeze" => {
            let age_seconds = finite(0).ok_or_else(invalid)?;
            if !(0.0..=120.0).contains(&age_seconds) {
                return Err(invalid());
            }
            Dev::BolidReplay {
                age_seconds,
                seed: integer(1),
                event_id: integer(2),
                frozen: op.path == "client.render.bolids.freeze",
            }
        }
        _ => return Err(invalid()),
    };
    Ok(PresentationOperation::Developer(developer))
}
