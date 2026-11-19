//! Built-in owner-lane administration reads and shutdown request.

use super::{
    ClientTrafficSnapshot, CommandPayload, CommandReply, PlayerRow, RuntimeCommandReadView,
    RuntimeStatusSnapshot, ScheduleDiagnostics, StatusReport,
};
use crate::{metrics::RuntimeMetrics, storage::PersistencePort, world_registry::WorldRegistry};
use minimium_gameplay::{CommandViewError, CommandViewRequest, ModuleCommandReadView};

pub(super) fn status(
    worlds: &WorldRegistry,
    lanes: usize,
    metrics: &RuntimeMetrics,
    schedule: ScheduleDiagnostics,
    persistence: &dyn PersistencePort,
    runtime: Option<RuntimeStatusSnapshot>,
) -> CommandReply {
    CommandReply::success(CommandPayload::Status(StatusReport {
        players: worlds.total_players(),
        lanes,
        ticks: metrics.ticks,
        clock_probe_requests: metrics.clock_probe_requests,
        clock_probe_accepted: metrics.clock_probe_accepted,
        clock_probe_rejected: metrics.clock_probe_rejected,
        environment_sync_messages: metrics.environment_sync_messages,
        scheduled_world_instances: schedule.world_instance_count,
        startup_system_runs: schedule.startup_system_runs,
        persistence: persistence.status(),
        runtime,
    }))
}

pub(super) fn players(worlds: &WorldRegistry, token: Option<&str>, limit: u16) -> CommandReply {
    let view = RuntimeCommandReadView::new(worlds);
    // Leave envelope and field-name headroom below the 32 KiB payload ceiling.
    let mut request = CommandViewRequest::new(usize::from(limit), 12 * 1024)
        .expect("validated player page limits");
    if let Some(token) = token {
        match view.parse_cursor(token) {
            Ok(cursor) => request = request.after(cursor),
            Err(_) => {
                return CommandReply::error(
                    "server.command.invalid_arguments",
                    "players cursor is invalid or stale",
                )
            }
        }
    }
    match view.players(&request) {
        Ok(page) => CommandReply::success(CommandPayload::Players {
            truncated: page.truncated(),
            next_cursor: page.next.as_ref().map(|cursor| cursor.token()),
            rows: page
                .rows
                .into_iter()
                .map(|player| PlayerRow {
                    id: player.id,
                    name: player.name,
                    world: player.world,
                    position: player.position,
                })
                .collect(),
        }),
        Err(CommandViewError::RecordTooLarge) => CommandReply::error(
            minimium_commands::CommandErrorCode::ResponseTooLarge.as_str(),
            "one player row exceeds the response budget",
        ),
        Err(_) => CommandReply::error("server.command.read_failed", "player page unavailable"),
    }
}

pub(super) fn metrics(
    worlds: &WorldRegistry,
    metrics: &RuntimeMetrics,
    traffic: ClientTrafficSnapshot,
) -> CommandReply {
    CommandReply::success(CommandPayload::Metrics {
        snapshot: metrics.snapshot(worlds.total_players()),
        clients: traffic.rows,
        clients_truncated: traffic.truncated,
    })
}

pub(super) fn stop() -> CommandReply {
    CommandReply::stopping(CommandPayload::Stopping)
}
