use super::{CommandId, CommandRegistry, CommandSpec};

const COMMANDS: &[CommandSpec] = &[
    CommandSpec {
        id: CommandId::HostLocal,
        name: "host local",
        alias: None,
        usage: "host local [--threads 1|2|auto]",
    },
    CommandSpec {
        id: CommandId::Connect,
        name: "connect",
        alias: None,
        usage: "connect HOST:PORT",
    },
    CommandSpec {
        id: CommandId::Disconnect,
        name: "disconnect",
        alias: None,
        usage: "disconnect",
    },
    CommandSpec {
        id: CommandId::Ping,
        name: "ping",
        alias: None,
        usage: "ping",
    },
];

pub(super) fn register(registry: &mut CommandRegistry) {
    registry.register(COMMANDS);
}
