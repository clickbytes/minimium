use super::{CommandId, CommandRegistry, CommandSpec};

const COMMANDS: &[CommandSpec] = &[
    CommandSpec {
        id: CommandId::SkyQuality,
        name: "sky quality",
        alias: None,
        usage: "sky quality low|standard",
    },
    CommandSpec {
        id: CommandId::SkyInfo,
        name: "sky info",
        alias: None,
        usage: "sky info",
    },
    CommandSpec {
        id: CommandId::TimeStatus,
        name: "time status",
        alias: None,
        usage: "time status",
    },
    CommandSpec {
        id: CommandId::TimeScale,
        name: "time scale",
        alias: Some("timescale"),
        usage: "time scale DECIMAL (alias timescale; owned local server; 0.1-1000000x or 0)",
    },
];

pub(super) fn register(registry: &mut CommandRegistry) {
    registry.register(COMMANDS);
}
