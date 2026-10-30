use super::{CommandId, CommandRegistry, CommandSpec};

const COMMANDS: &[CommandSpec] = &[
    CommandSpec {
        id: CommandId::Move,
        name: "move",
        alias: None,
        usage: "move X Z",
    },
    CommandSpec {
        id: CommandId::Jump,
        name: "jump",
        alias: None,
        usage: "jump",
    },
    CommandSpec {
        id: CommandId::UsePortal,
        name: "portal",
        alias: None,
        usage: "portal",
    },
    CommandSpec {
        id: CommandId::MapTeleport,
        name: "map_telepo",
        alias: None,
        usage: "map_telepo: click land on the Earth overview",
    },
    CommandSpec {
        id: CommandId::Fire,
        name: "fire",
        alias: None,
        usage: "fire",
    },
    CommandSpec {
        id: CommandId::Chat,
        name: "say",
        alias: Some("chat"),
        usage: "say TEXT (alias: chat)",
    },
    CommandSpec {
        id: CommandId::SetName,
        name: "name",
        alias: None,
        usage: "name NAME",
    },
];

pub(super) fn register(registry: &mut CommandRegistry) {
    registry.register(COMMANDS);
}
