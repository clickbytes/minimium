use crate::commands::{BoundedText, CommandReply, PropertyReport, StatusReport};
use crate::hosting::replies::{CommandPayloadView, CommandReplyView};
use std::fmt::Write as _;

pub(crate) fn render(payload: &CommandPayloadView<'_>) -> BoundedText {
    let mut out = BoundedText::default();
    match payload {
        CommandPayloadView::ModuleText { text, .. } => out.push_str(text),
        CommandPayloadView::SaveQueued => {
            out.push_str("world save queued; use status to check completion")
        }
        CommandPayloadView::Stopping => out.push_str("stopping"),
        CommandPayloadView::Property(report) => property(&mut out, report),
        CommandPayloadView::PropertyHelp(spec) => {
            let _ = write!(out, "{}: {:?}; default {}; constraints {}; owner {}; permission {:?}; lifecycle {:?}; persistent {}; resettable {}\n{}", spec.key, spec.value_type, spec.default.unwrap_or("n/a"), spec.constraints.unwrap_or("none"), spec.owner, spec.permission, spec.lifecycle, spec.persistent, spec.resettable, spec.description);
        }
        CommandPayloadView::PropertyCommandsHelp { builtins } => {
            for (i, line) in builtins.iter().enumerate() {
                if i != 0 {
                    out.push_str(" | ");
                }
                out.push_str(line);
            }
            out.push_str("\nget KEY | set KEY VALUE | reset KEY | list [PREFIX]");
        }
        CommandPayloadView::SingleCommandHelp(help) => {
            out.push_str(help.description());
        }
        CommandPayloadView::PropertyList {
            prefix,
            rows,
            truncated,
        } => {
            if rows.is_empty() {
                let _ = write!(
                    out,
                    "no server properties match {}",
                    prefix.as_deref().unwrap_or("the requested prefix")
                );
            }
            for (i, row) in rows.iter().enumerate() {
                if i != 0 {
                    out.push_str("\n");
                }
                let _ = write!(
                    out,
                    "{}: {}",
                    row.key,
                    row.constraints.unwrap_or("no constraints")
                );
            }
            if *truncated {
                out.push_str("\n[truncated]");
            }
        }
        CommandPayloadView::Status(status) => status_output(&mut out, status),
        CommandPayloadView::Players {
            rows: players,
            truncated,
            next_cursor,
        } => {
            if let Some(cursor) = next_cursor {
                let _ = write!(out, "next={cursor}; ");
            }
            if players.is_empty() {
                out.push_str("no players");
            }
            for (index, player) in players.iter().enumerate() {
                if index != 0 {
                    out.push_str("; ");
                }
                let _ = write!(
                    out,
                    "{}:{}:world={}@{:.1},{:.1},{:.1}",
                    player.id,
                    player.name,
                    player.world,
                    player.position[0],
                    player.position[1],
                    player.position[2]
                );
            }
            if *truncated {
                if next_cursor.is_none() {
                    out.push_str("; [truncated]");
                }
            }
        }
        CommandPayloadView::WorldList {
            rows,
            truncated,
            next_cursor,
        } => {
            if let Some(cursor) = next_cursor {
                let _ = write!(out, "next={cursor}; ");
            }
            for (i, row) in rows.iter().enumerate() {
                if i != 0 {
                    out.push_str("; ");
                }
                let _ = write!(
                    out,
                    "{}: players={} topology={:?} generation={:?}",
                    row.id, row.players, row.topology, row.generation
                );
            }
            if *truncated {
                if next_cursor.is_none() {
                    out.push_str("; [truncated]");
                }
            }
        }
        CommandPayloadView::WorldInspect(row) => {
            let _ = write!(
                out,
                "{}: players={} manifest={:?}",
                row.id, row.players, row.manifest
            );
        }
        CommandPayloadView::EntityList {
            rows,
            truncated,
            next_cursor,
        } => {
            if let Some(cursor) = next_cursor {
                let _ = write!(out, "next={cursor}; ");
            }
            if rows.is_empty() {
                out.push_str("no runtime entities or players");
            }
            for (i, row) in rows.iter().enumerate() {
                if i != 0 {
                    out.push_str("; ");
                }
                match &row.kind {
                    crate::commands::EntityKind::Entity(id) => {
                        let _ = write!(out, "world={} entity={id}", row.world);
                    }
                    crate::commands::EntityKind::Player(id, name) => {
                        let _ = write!(out, "world={} player={id}:{name}", row.world);
                    }
                }
            }
            if *truncated {
                if next_cursor.is_none() {
                    out.push_str("; [truncated]");
                }
            }
        }
        CommandPayloadView::EntityInspect {
            world,
            entity,
            rows,
            truncated,
            next_cursor,
        } => {
            if let Some(cursor) = next_cursor {
                let _ = write!(out, "next={cursor}; ");
            }
            let _ = write!(out, "world={world} entity={}", entity.get());
            if rows.is_empty() {
                out.push_str(" components=none");
            }
            for row in rows.iter() {
                let _ = write!(
                    out,
                    " component={}:schema={}:bytes={}",
                    row.kind.get(),
                    row.schema.get(),
                    row.bytes
                );
            }
            if *truncated {
                out.push_str(" [truncated]");
            }
        }
        CommandPayloadView::EntityDespawn {
            world,
            entity,
            revision,
        } => {
            let _ = write!(out, "world={world} entity={} despawn effect", entity.get());
            if let Some(revision) = revision {
                let _ = write!(out, " committed revision={revision}");
            }
        }
        CommandPayloadView::Bolid {
            id,
            kind,
            recipients,
            shower,
            world,
            observer,
            tick,
            ..
        } => {
            let _ = write!(out, "bolid id={id} world={world} observer={} tick={tick} kind={kind} recipients={recipients}", observer.get());
            if let Some(count) = shower {
                let _ = write!(
                    out,
                    "; shower accepted: {count} bolids total, first now, remaining {} at irregular 0.9..8 s intervals",
                    count.saturating_sub(1)
                );
            }
        }
        CommandPayloadView::ImpactSpawn {
            id,
            world,
            observer,
            tick,
            ..
        } => {
            let _ = write!(
                out,
                "meteorid {id} reserved world={world} observer={} tick={tick}; checkpoint pending",
                observer.get()
            );
        }
        CommandPayloadView::BolidStatus(status) => {
            let _ = write!(out, "shower id={} state={} requested={} emitted={} remaining={} world={} observer={} started_tick={}",
                status.id(), status.state().label(), status.requested(), status.emitted(), status.remaining(),
                status.world(), status.observer().get(), status.started_tick());
            if let Some(tick) = status.terminal_tick() {
                let _ = write!(out, " terminal_tick={tick}");
            }
            if let Some(reason) = status.reason() {
                let _ = write!(out, " reason={reason}");
            }
        }
        CommandPayloadView::ImpactReceipt(receipt) => {
            let _ = write!(
                out,
                "impact id={} world={} observer={} stage={} requested_tick={}",
                receipt.id(),
                receipt.world(),
                receipt.observer().get(),
                receipt.stage().label(),
                receipt.requested_tick()
            );
            if let Some(revision) = receipt.journal_revision() {
                let _ = write!(out, " journal_revision={revision}");
            }
            if let Some(tick) = receipt.committed_tick() {
                let _ = write!(out, " committed_tick={tick}");
            }
            if let Some(reason) = receipt.reason() {
                let _ = write!(out, " reason={reason}");
            }
        }
        CommandPayloadView::Help {
            builtins,
            modules,
            truncated,
        } => {
            for (i, row) in builtins.iter().enumerate() {
                if i != 0 {
                    out.push_str(" | ");
                }
                out.push_str(row);
            }
            for row in modules.iter() {
                if !out.as_str().is_empty() {
                    out.push_str(" | ");
                }
                let _ = minimium_commands::write_command_help(&mut out, row.usage, row.arguments);
            }
            if *truncated {
                out.push_str(" | [truncated]");
            }
        }
        CommandPayloadView::Metrics {
            snapshot,
            clients,
            clients_truncated,
        } => {
            for field in &snapshot.fields {
                let value = match &field.value {
                    crate::metrics::MetricValue::Count(v) => v.to_string(),
                    crate::metrics::MetricValue::Optional(v) => {
                        (*v).map_or_else(|| "n/a".to_owned(), |v| v.to_string())
                    }
                    crate::metrics::MetricValue::Ratio(v) => format!("{v:.2}"),
                    crate::metrics::MetricValue::Label(v) => (*v).to_owned(),
                };
                let _ = write!(
                    out,
                    "{}{}={}",
                    if out.as_str().is_empty() { "" } else { " " },
                    field.name,
                    value
                );
            }
            out.push_str(" client_traffic=id:bytes_in:bytes_out:decoded_frames:queued_frames:encoding_us:connected_ms");
            for row in clients.iter() {
                let _ = write!(
                    out,
                    " {}:{}:{}:{}:{}:{}:{}",
                    row.id,
                    row.bytes_in,
                    row.bytes_out,
                    row.decoded_frames,
                    row.queued_frames,
                    row.encoding_us,
                    row.connected_ms
                );
            }
            if *clients_truncated {
                out.push_str(" [truncated]");
            }
        }
        CommandPayloadView::ClockCommitted(snapshot) | CommandPayloadView::Time(snapshot) => {
            let nonce: String = snapshot
                .session_nonce
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect();
            let correction = snapshot
                .correction_ms
                .map(|value| format!("{value:+}"))
                .unwrap_or_else(|| "none".to_owned());
            let dirty = snapshot
                .dirty_revision
                .map(|value| value.to_string())
                .unwrap_or_else(|| "none".to_owned());
            let _ = write!(out, "clock_id={} revision={} session_nonce={} mode={} world_utc_ms={} rate_ppm={} rate={} offline_policy={} fault={} wall_drift_ms={} sync_dirty_revision={} tick={} achieved_ppm={} achieved_rate={} paused={} limit={} calendar_epoch={}", snapshot.clock_id, snapshot.revision, nonce, snapshot.mode, snapshot.utc_ms, snapshot.rate_ppm, format_clock_scale(snapshot.rate_ppm), snapshot.offline_policy, snapshot.fault, correction, dirty, snapshot.tick, snapshot.achieved_ppm, format_clock_scale(snapshot.achieved_ppm), snapshot.rate_ppm == 0, snapshot.limit, snapshot.calendar_epoch());
        }
    }
    out
}

pub(crate) fn render_reply(reply: &CommandReply) -> BoundedText {
    let view = reply.view();
    render_reply_view(&view)
}

pub(crate) fn render_reply_view(view: &CommandReplyView<'_>) -> BoundedText {
    let mut out = view
        .payload
        .as_ref()
        .map(render)
        .unwrap_or_else(|| BoundedText::new(view.detail.unwrap_or("command failed")));
    if let Some(help) = view.catalog_help {
        out.push_str("\n");
        out.push_str(help);
    }
    if view.payload.is_some() {
        if let Some(outcome) = view.effects {
            if !out.as_str().is_empty() {
                out.push_str("; ");
            }
            out.push_str("effects=");
            for (index, result) in outcome.results().iter().enumerate() {
                if index > 0 {
                    out.push_str(",");
                }
                let _ = write!(out, "{index}:");
                match result {
                    crate::modules::WorldEffectOutcome::EntityApplied => {
                        out.push_str("entity_applied")
                    }
                    crate::modules::WorldEffectOutcome::EntitySpawned { .. } => {
                        out.push_str("entity_applied")
                    }
                    crate::modules::WorldEffectOutcome::EntityRejected(reason) => {
                        let _ = write!(out, "entity_rejected_{}", entity_rejection_label(*reason));
                    }
                    crate::modules::WorldEffectOutcome::Transfer(
                        crate::modules::TransferOutcome::Started,
                    ) => out.push_str(
                        "transfer_started_prepare_queued_readiness_pending_not_committed",
                    ),
                    crate::modules::WorldEffectOutcome::Transfer(
                        crate::modules::TransferOutcome::Rejected(reason),
                    ) => {
                        let _ = write!(
                            out,
                            "transfer_rejected_{}",
                            transfer_rejection_label(*reason)
                        );
                    }
                    crate::modules::WorldEffectOutcome::StageRejected => {
                        out.push_str("stage_rejected")
                    }
                }
            }
            if outcome.rejected_pushes() > 0 {
                let _ = write!(out, ",rejected_pushes={}", outcome.rejected_pushes());
            }
            if outcome.has_rejections() {
                out.push_str(" [not all effects applied]");
            }
        }
    }
    out
}

pub(crate) fn entity_rejection_label(
    reason: crate::modules::EntityEffectRejection,
) -> &'static str {
    match reason {
        crate::modules::EntityEffectRejection::Validation => "validation",
        crate::modules::EntityEffectRejection::StorageBudget => "storage_budget",
        crate::modules::EntityEffectRejection::UnknownInstance => "unknown_instance",
        crate::modules::EntityEffectRejection::UnknownEntity => "unknown_entity",
        crate::modules::EntityEffectRejection::ProtectedComponent => "protected_component",
        crate::modules::EntityEffectRejection::ReservationConflict => "reservation_conflict",
        crate::modules::EntityEffectRejection::IdentifierSpaceExhausted => {
            "identifier_space_exhausted"
        }
    }
}

pub(crate) fn transfer_rejection_label(reason: crate::modules::TransferRejection) -> &'static str {
    match reason {
        crate::modules::TransferRejection::Validation => "validation",
        crate::modules::TransferRejection::AlreadyPending => "already_pending",
        crate::modules::TransferRejection::IdentifierSpaceExhausted => "identifier_space_exhausted",
        crate::modules::TransferRejection::ClientUnavailable => "client_unavailable",
        crate::modules::TransferRejection::EnvironmentUnavailable => "environment_unavailable",
        crate::modules::TransferRejection::Backpressure => "backpressure",
    }
}

pub(crate) fn format_clock_scale(rate: u64) -> String {
    format!("{}.{:06}x", rate / 1_000_000, rate % 1_000_000)
}

fn property(out: &mut BoundedText, report: &PropertyReport) {
    let _ = write!(
        out,
        "{} {}",
        report.key,
        report.value.format_with(String::clone)
    );
}

fn status_output(out: &mut BoundedText, status: &StatusReport) {
    let save = match &status.persistence.last_save {
        minimium_storage::SaveStatus::Never => "never".to_owned(),
        minimium_storage::SaveStatus::Pending => "pending".to_owned(),
        minimium_storage::SaveStatus::Complete => "ok".to_owned(),
        minimium_storage::SaveStatus::Failed(message) => format!("error_{message}"),
    };
    let _ = write!(out, "running players={} lanes={} ticks={} clock_probe_requests={} clock_probe_accepted={} clock_probe_rejected={} environment_sync_messages={} scheduled_world_instances={} startup_system_runs={} save_pending={} save_last={}", status.players, status.lanes, status.ticks, status.clock_probe_requests, status.clock_probe_accepted, status.clock_probe_rejected, status.environment_sync_messages, status.scheduled_world_instances, status.startup_system_runs, status.persistence.pending_saves, save);
    let Some(runtime) = &status.runtime else {
        return;
    };
    let _ = write!(out, " game_address={} max_players={} map_fingerprint={:016x} tick_ms={} snapshot_ms={} autosave_seconds={} protocol_version={} save_version={} environment_profiles=", runtime.game_address, runtime.max_players, runtime.map_fingerprint, runtime.tick_ms, runtime.snapshot_ms, runtime.autosave_seconds, runtime.protocol_version, runtime.save_version);
    for (i, (id, clock, profile)) in runtime.environment_profiles.iter().enumerate() {
        if i != 0 {
            out.push_str(",");
        }
        let _ = write!(out, "{id}:{clock}:{profile:?}");
    }
    if runtime.environment_profiles_truncated {
        out.push_str(",[truncated]");
    }
    let impact = runtime.impact;
    let _ = write!(
        out,
        " meteorids_active={} impact_craters={} impact_pending={} impact_storage={}",
        impact.active,
        impact.craters,
        impact.pending,
        if impact.ready { "ready" } else { "failed" }
    );
    let excavation = &runtime.excavation;
    let _ = write!(out, " excavation_chunks={} excavation_pending={} excavation_bytes={} excavation_storage={} fluid_cells={} fluid_jobs={} terrain_payload_bytes={} fluid_payload_bytes={} fluid_reserved={} fluid_failed_worlds={}", excavation.chunks, excavation.pending, excavation.bytes, if excavation.ready {"ready"} else {"failed"}, excavation.fluid_cells, excavation.fluid_jobs, excavation.terrain_payload_bytes, excavation.fluid_payload_bytes, excavation.fluid_reserved, excavation.failed_worlds);
    for (name, p95, p99) in &excavation.percentiles {
        let _ = write!(
            out,
            " excavation_{name}_p95_us={p95:?} excavation_{name}_p99_us={p99:?}"
        );
    }
}
