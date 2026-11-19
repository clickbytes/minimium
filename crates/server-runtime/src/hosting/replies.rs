//! Read-only, bounded command reply data for runtime hosts.
use crate::commands::{CommandPayload, CommandReply};

/// Execution state visible to an embedded host.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReplyState {
    Rejected,
    Executed,
    Partial,
}

/// Immutable view of one command result. `should_stop` is independent of state.
pub struct CommandReplyView<'a> {
    pub state: ReplyState,
    pub should_stop: bool,
    pub error_code: Option<&'static str>,
    pub detail: Option<&'a str>,
    pub catalog_help: Option<&'a str>,
    pub payload: Option<CommandPayloadView<'a>>,
    pub effects: Option<&'a minimium_gameplay::EffectBatchOutcome>,
}

/// Exhaustive read-only projection of the runtime's existing command payloads.
pub enum CommandPayloadView<'a> {
    Help {
        builtins: &'a [&'static str],
        modules: &'a [crate::commands::CommandHelp],
        truncated: bool,
    },
    Status(&'a crate::commands::StatusReport),
    Players {
        rows: &'a [crate::commands::PlayerRow],
        truncated: bool,
        next_cursor: Option<&'a str>,
    },
    Metrics {
        snapshot: &'a crate::metrics::RuntimeMetricsSnapshot,
        clients: &'a [crate::commands::ClientTrafficRow],
        clients_truncated: bool,
    },
    SaveQueued,
    Stopping,
    Time(&'a crate::ClockPropertySnapshot),
    Property(&'a crate::commands::PropertyReport),
    UnknownProperty(&'a str),
    UnavailableProperty {
        key: &'a str,
        reason: &'a str,
    },
    PropertyHelp(&'a minimium_commands::PropertySpec),
    PropertyCommandsHelp {
        builtins: &'a [&'static str],
    },
    SingleCommandHelp(&'a crate::commands::CommandHelp),
    PropertyList {
        prefix: Option<&'a str>,
        rows: &'a [minimium_commands::PropertySpec],
        truncated: bool,
    },
    WorldList {
        rows: &'a [crate::commands::WorldRow],
        truncated: bool,
        next_cursor: Option<&'a str>,
    },
    WorldInspect(&'a crate::commands::WorldInspect),
    EntityList {
        rows: &'a [crate::commands::EntityRow],
        truncated: bool,
        next_cursor: Option<&'a str>,
    },
    EntityInspect {
        world: minimium_world::world::WorldInstanceId,
        entity: minimium_world::entity::EntityId,
        rows: &'a [crate::commands::EntityComponentRow],
        truncated: bool,
        next_cursor: Option<&'a str>,
    },
    EntityDespawn {
        world: minimium_world::world::WorldInstanceId,
        entity: minimium_world::entity::EntityId,
    },
    Bolid {
        id: u64,
        kind: u8,
        recipients: usize,
        shower: Option<usize>,
    },
    ImpactSpawn {
        id: u64,
    },
    ModuleText {
        text: &'a str,
        truncated: bool,
    },
}

impl CommandReply {
    /// Borrows the complete result without exposing runtime mutation handles.
    pub fn view(&self) -> CommandReplyView<'_> {
        let payload = self.payload().map(|payload| match payload {
            CommandPayload::Help {
                builtins,
                modules,
                truncated,
            } => CommandPayloadView::Help {
                builtins,
                modules,
                truncated: *truncated,
            },
            CommandPayload::Status(value) => CommandPayloadView::Status(value),
            CommandPayload::Players {
                rows,
                truncated,
                next_cursor,
            } => CommandPayloadView::Players {
                rows,
                truncated: *truncated,
                next_cursor: next_cursor.as_deref(),
            },
            CommandPayload::Metrics {
                snapshot,
                clients,
                clients_truncated,
            } => CommandPayloadView::Metrics {
                snapshot,
                clients,
                clients_truncated: *clients_truncated,
            },
            CommandPayload::SaveQueued => CommandPayloadView::SaveQueued,
            CommandPayload::Stopping => CommandPayloadView::Stopping,
            CommandPayload::Time(value) => CommandPayloadView::Time(value),
            CommandPayload::Property(value) => CommandPayloadView::Property(value),
            CommandPayload::UnknownProperty(key) => CommandPayloadView::UnknownProperty(key),
            CommandPayload::UnavailableProperty { key, reason } => {
                CommandPayloadView::UnavailableProperty { key, reason }
            }
            CommandPayload::PropertyHelp(value, _) => CommandPayloadView::PropertyHelp(value),
            CommandPayload::PropertyCommandsHelp { builtins } => {
                CommandPayloadView::PropertyCommandsHelp { builtins }
            }
            CommandPayload::SingleCommandHelp(value) => {
                CommandPayloadView::SingleCommandHelp(value)
            }
            CommandPayload::PropertyList {
                prefix,
                rows,
                truncated,
            } => CommandPayloadView::PropertyList {
                prefix: prefix.as_deref(),
                rows,
                truncated: *truncated,
            },
            CommandPayload::WorldList {
                rows,
                truncated,
                next_cursor,
            } => CommandPayloadView::WorldList {
                rows,
                truncated: *truncated,
                next_cursor: next_cursor.as_deref(),
            },
            CommandPayload::WorldInspect(value) => CommandPayloadView::WorldInspect(value),
            CommandPayload::EntityList {
                rows,
                truncated,
                next_cursor,
            } => CommandPayloadView::EntityList {
                rows,
                truncated: *truncated,
                next_cursor: next_cursor.as_deref(),
            },
            CommandPayload::EntityInspect {
                world,
                entity,
                rows,
                truncated,
                next_cursor,
            } => CommandPayloadView::EntityInspect {
                world: *world,
                entity: *entity,
                rows,
                truncated: *truncated,
                next_cursor: next_cursor.as_deref(),
            },
            CommandPayload::EntityDespawn { world, entity } => CommandPayloadView::EntityDespawn {
                world: *world,
                entity: *entity,
            },
            CommandPayload::Bolid {
                id,
                kind,
                recipients,
                shower,
            } => CommandPayloadView::Bolid {
                id: *id,
                kind: *kind,
                recipients: *recipients,
                shower: *shower,
            },
            CommandPayload::ImpactSpawn { id } => CommandPayloadView::ImpactSpawn { id: *id },
            CommandPayload::ModuleText(value) => CommandPayloadView::ModuleText {
                text: value.as_str(),
                truncated: value.is_truncated(),
            },
        });
        let state = if !self.handler_succeeded() {
            ReplyState::Rejected
        } else if self
            .effect_outcome()
            .is_some_and(|outcome| outcome.has_rejections() && outcome.has_successes())
        {
            ReplyState::Partial
        } else if self
            .effect_outcome()
            .is_some_and(minimium_gameplay::EffectBatchOutcome::has_rejections)
        {
            ReplyState::Rejected
        } else {
            ReplyState::Executed
        };
        CommandReplyView {
            state,
            should_stop: self.should_stop(),
            error_code: self.error_code(),
            detail: self.detail(),
            catalog_help: match self.payload() {
                Some(CommandPayload::PropertyHelp(_, description)) => Some(description),
                _ => None,
            },
            payload,
            effects: self.effect_outcome(),
        }
    }
}
