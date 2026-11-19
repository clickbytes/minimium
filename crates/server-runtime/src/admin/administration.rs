//! Bounded typed projections of the existing owner-lane admin snapshots.

use crate::hosting::replies::CommandPayloadView as View;
use minimium_commands::operation::{CommandValue as V, FiniteValue};

fn record(fields: impl IntoIterator<Item = (&'static str, V)>) -> V {
    V::Record(
        fields
            .into_iter()
            .map(|(key, value)| (key.into(), value))
            .collect(),
    )
}

pub(super) fn project(view: &View<'_>) -> Option<V> {
    Some(match view {
        View::Status(status) => {
            let save = match &status.persistence().last_save {
                minimium_storage::SaveStatus::Never => "never",
                minimium_storage::SaveStatus::Pending => "pending",
                minimium_storage::SaveStatus::Complete => "complete",
                minimium_storage::SaveStatus::Failed(_) => "failed",
            };
            let mut fields = vec![
                ("players", V::U64(status.players() as u64)),
                ("lanes", V::U64(status.lanes() as u64)),
                ("ticks", V::U64(status.ticks())),
                (
                    "clock_probe_requests",
                    V::U64(status.clock_probe_requests()),
                ),
                (
                    "clock_probe_accepted",
                    V::U64(status.clock_probe_accepted()),
                ),
                (
                    "clock_probe_rejected",
                    V::U64(status.clock_probe_rejected()),
                ),
                (
                    "environment_sync_messages",
                    V::U64(status.environment_sync_messages()),
                ),
                (
                    "scheduled_world_instances",
                    V::U64(status.scheduled_world_instances() as u64),
                ),
                (
                    "startup_system_runs",
                    V::U64(status.startup_system_runs() as u64),
                ),
                (
                    "pending_saves",
                    V::U64(status.persistence().pending_saves as u64),
                ),
                ("last_save", V::Enum(save.into())),
            ];
            if let Some(runtime) = status.runtime() {
                fields.extend([
                    ("game_address", V::Text(runtime.game_address().into())),
                    ("max_players", V::U64(runtime.max_players() as u64)),
                    ("tick_ms", V::U128(runtime.tick_ms())),
                    ("snapshot_ms", V::U128(runtime.snapshot_ms())),
                    ("autosave_seconds", V::U64(runtime.autosave_seconds())),
                    (
                        "protocol_version",
                        V::U64(u64::from(runtime.protocol_version())),
                    ),
                    ("save_version", V::U64(u64::from(runtime.save_version()))),
                    ("map_fingerprint", V::U64(runtime.map_fingerprint())),
                    ("impact_active", V::U64(runtime.impact().active as u64)),
                    ("impact_craters", V::U64(runtime.impact().craters as u64)),
                    ("impact_pending", V::U64(runtime.impact().pending as u64)),
                    ("impact_ready", V::Bool(runtime.impact().ready)),
                    (
                        "excavation_chunks",
                        V::U64(runtime.excavation().chunks() as u64),
                    ),
                    (
                        "excavation_pending",
                        V::U64(runtime.excavation().pending() as u64),
                    ),
                    ("excavation_bytes", V::U64(runtime.excavation().bytes())),
                    ("excavation_ready", V::Bool(runtime.excavation().ready())),
                    (
                        "fluid_cells",
                        V::U64(runtime.excavation().fluid_cells() as u64),
                    ),
                    (
                        "fluid_jobs",
                        V::U64(runtime.excavation().fluid_jobs() as u64),
                    ),
                    (
                        "environment_profiles",
                        V::U64(runtime.environment_profiles().len() as u64),
                    ),
                    (
                        "environment_profiles_truncated",
                        V::Bool(runtime.environment_profiles_truncated()),
                    ),
                ]);
            }
            record(fields)
        }
        View::Metrics {
            snapshot,
            clients,
            clients_truncated,
        } => {
            let fields = snapshot
                .fields()
                .iter()
                .take(128)
                .map(|field| {
                    let value = match field.value() {
                        crate::MetricValue::Count(value) => V::U64(*value),
                        crate::MetricValue::Optional(Some(value)) => V::U64(*value),
                        crate::MetricValue::Optional(None) => V::Unit,
                        crate::MetricValue::Ratio(value) => {
                            FiniteValue::new(*value).map_or(V::Unit, V::Finite)
                        }
                        crate::MetricValue::Label(value) => V::Enum((*value).into()),
                    };
                    record([("name", V::Text(field.name().into())), ("value", value)])
                })
                .collect();
            let traffic = clients
                .iter()
                .take(64)
                .map(|client| {
                    record([
                        ("id", V::U64(client.id().get())),
                        ("bytes_in", V::U64(client.bytes_in())),
                        ("bytes_out", V::U64(client.bytes_out())),
                        ("decoded_frames", V::U64(client.decoded_frames())),
                        ("queued_frames", V::U64(client.queued_frames())),
                        ("encoding_us", V::U64(client.encoding_us())),
                        ("connected_ms", V::U128(client.connected_ms())),
                    ])
                })
                .collect();
            record([
                (
                    "fields",
                    V::Page {
                        rows: fields,
                        next_cursor: None,
                    },
                ),
                ("fields_truncated", V::Bool(snapshot.fields().len() > 128)),
                (
                    "clients",
                    V::Page {
                        rows: traffic,
                        next_cursor: None,
                    },
                ),
                (
                    "clients_truncated",
                    V::Bool(*clients_truncated || clients.len() > 64),
                ),
            ])
        }
        View::Stopping => record([
            ("shutdown_request", V::Enum("owner_committed".into())),
            ("process_finished", V::Bool(false)),
            ("save_finished", V::Bool(false)),
        ]),
        _ => return None,
    })
}
