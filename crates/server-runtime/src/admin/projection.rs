use crate::hosting::replies::{CommandPayloadView as View, CommandReplyView};
use crate::server_command::{ServerCommand, ServerPropertyCommand};
use minimium_commands::operation::{CommandValue as V, OperationCatalog};
use minimium_commands::results::Payload;

pub(super) fn payload(
    view: &CommandReplyView<'_>,
    command: &ServerCommand,
    catalog: &OperationCatalog,
) -> Payload {
    let paths = |names: Vec<String>, next_cursor| V::Page {
        rows: names.into_iter().map(V::Text).collect(),
        next_cursor,
    };
    let value = match view.payload.as_ref() {
        Some(View::Time(clock)) => Some(clock.status().value()),
        Some(View::ClockCommitted(clock)) => Some(clock_receipt(clock, command)),
        Some(View::PropertyHelp(spec)) => catalog
            .resolve(spec.key)
            .map(minimium_commands::admin::metadata::describe),
        Some(View::SingleCommandHelp(help)) => match command {
            ServerCommand::Property(ServerPropertyCommand::Help(Some(target))) => catalog
                .resolve(target)
                .map(minimium_commands::admin::metadata::describe)
                .or_else(|| {
                    Some(V::Record(
                        [
                            ("usage".into(), V::Text(help.usage().into())),
                            ("description".into(), V::Text(help.description().into())),
                        ]
                        .into(),
                    ))
                }),
            _ => None,
        },
        Some(View::Help { .. } | View::PropertyCommandsHelp { .. }) => Some(paths(
            catalog.entries().map(|e| e.spec.path.clone()).collect(),
            None,
        )),
        Some(View::PropertyList {
            rows,
            prefix,
            truncated,
        }) => Some(V::Record(
            [
                (
                    "rows".into(),
                    paths(rows.iter().map(|s| s.key.to_owned()).collect(), None),
                ),
                (
                    "prefix".into(),
                    prefix.map_or(V::Unit, |s| V::Text(s.into())),
                ),
                ("truncated".into(), V::Bool(*truncated)),
            ]
            .into(),
        )),
        Some(View::Property(r)) => return Payload::Property(r.typed_value()),
        Some(View::WaterStatus(status)) => Some(crate::commands::water::project(status)),
        Some(View::SaveJob(job)) => Some(crate::commands::persistence::project_job(job)),
        Some(View::SaveStatus(status)) => {
            Some(crate::commands::persistence::project_status(status))
        }
        Some(View::EntityDespawn {
            world,
            entity,
            revision,
        }) => Some(V::Record(
            [
                ("source".into(), V::Enum("owner_snapshot".into())),
                ("instance_id".into(), V::U128(world.get())),
                ("entity_id".into(), V::U64(entity.get())),
                ("revision".into(), revision.map_or(V::Unit, V::U64)),
                (
                    "state".into(),
                    V::Enum(
                        if view.state == crate::hosting::replies::ReplyState::Rejected {
                            "rejected"
                        } else if revision.is_some() {
                            "owner_committed"
                        } else {
                            "owner_accepted"
                        }
                        .into(),
                    ),
                ),
            ]
            .into(),
        )),
        Some(
            v @ (View::Bolid { .. }
            | View::BolidStatus(_)
            | View::ImpactSpawn { .. }
            | View::ImpactReceipt(_)),
        ) => crate::commands::bolids::project(v),
        Some(View::Status(_) | View::Metrics { .. } | View::Stopping) => view
            .payload
            .as_ref()
            .and_then(super::administration::project),
        other => other.and_then(super::pages::project),
    };
    value.map(Payload::Value).unwrap_or_else(|| {
        Payload::LegacyText(
            crate::command_output::render_reply_view(view)
                .as_str()
                .into(),
        )
    })
}

fn clock_receipt(clock: &crate::ClockPropertySnapshot, command: &ServerCommand) -> V {
    use minimium_environment::{
        time::{ClockMode, OfflinePolicy},
        TimeCommand,
    };
    let status = clock.status();
    let (requested, path) = match command {
        ServerCommand::Time(TimeCommand::Scale(rate)) => {
            (V::decimal(i128::from(rate.get()), 6), "world.time.scale")
        }
        ServerCommand::Time(TimeCommand::Mode(mode)) => (
            V::Enum(
                match mode {
                    ClockMode::TimeScale => "time_scale",
                    ClockMode::RealTime => "real_time",
                }
                .into(),
            ),
            "world.time.mode",
        ),
        ServerCommand::Time(TimeCommand::Offline(policy)) => (
            V::Enum(
                match policy {
                    OfflinePolicy::Pause => "pause",
                    OfflinePolicy::Advance => "advance",
                }
                .into(),
            ),
            "world.time.offline_policy",
        ),
        ServerCommand::Time(TimeCommand::Set(instant)) => {
            (V::InstantMs(instant.get()), "world.time.utc")
        }
        ServerCommand::TimeOfDay { minute, .. } => (
            V::Enum(format!("{:02}{:02}", minute / 60, minute % 60)),
            "world.time.utc",
        ),
        _ => return status.value(),
    };
    let V::Record(mut fields) = status.value() else {
        unreachable!("clock status record")
    };
    fields.insert("requested".into(), requested);
    fields.insert(
        "effective".into(),
        status.property(path).expect("clock property"),
    );
    V::Record(fields)
}
