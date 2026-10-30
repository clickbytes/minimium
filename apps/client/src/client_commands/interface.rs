use super::{CommandId, CommandRegistry, CommandSpec};

const COMMANDS: &[CommandSpec] = &[
    CommandSpec {
        id: CommandId::Help,
        name: "help",
        alias: None,
        usage: "help",
    },
    CommandSpec {
        id: CommandId::FpsShow,
        name: "fps show",
        alias: None,
        usage: "fps show",
    },
    CommandSpec {
        id: CommandId::NetStats,
        name: "net stats",
        alias: None,
        usage: "net stats",
    },
    CommandSpec {
        id: CommandId::State,
        name: "state",
        alias: None,
        usage: "state",
    },
    CommandSpec {
        id: CommandId::Clear,
        name: "clear",
        alias: None,
        usage: "clear",
    },
    CommandSpec {
        id: CommandId::Quit,
        name: "quit",
        alias: Some("exit"),
        usage: "quit (alias: exit)",
    },
];

pub(super) fn register(registry: &mut CommandRegistry) {
    registry.register(COMMANDS);
}
