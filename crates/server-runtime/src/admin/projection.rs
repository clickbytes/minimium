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
        Some(View::Property(r)) => return Payload::Property(r.value().clone().map(V::Text)),
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
