use minimium_server_runtime::*;

pub(super) fn inspect_reply(reply: CommandReplyView<'_>) {
    if reply.state == ReplyState::Rejected {
        println!("error {:?}: {:?}", reply.error_code, reply.detail);
    }
    if reply.state == ReplyState::Partial {
        if let Some(effects) = reply.effects {
            for effect in effects.results() {
                println!("effect {effect:?}");
            }
        }
    }
    if reply.should_stop {
        println!("runtime requested stop");
    }
    let Some(payload) = reply.payload else {
        return;
    };
    match payload {
        CommandPayloadView::Help {
            builtins,
            modules,
            truncated,
        } => {
            for command in builtins {
                println!("builtin {command}");
            }
            for module in modules {
                println!("{} {} args", module.usage(), module.arguments().len());
            }
            println!("help truncated={truncated}");
        }
        CommandPayloadView::Status(status) => {
            println!(
                "status players={} lanes={} ticks={}",
                status.players(),
                status.lanes(),
                status.ticks()
            );
            println!(
                "clock probes={}/{}/{} sync={} worlds={} startup={}",
                status.clock_probe_requests(),
                status.clock_probe_accepted(),
                status.clock_probe_rejected(),
                status.environment_sync_messages(),
                status.scheduled_world_instances(),
                status.startup_system_runs()
            );
            println!(
                "persistence pending={} last={:?}",
                status.persistence().pending_saves,
                status.persistence().last_save
            );
            if let Some(runtime) = status.runtime() {
                println!(
                    "runtime {} players={} tick={} snapshot={} autosave={} protocol={} save={} fingerprint={}",
                    runtime.game_address(),
                    runtime.max_players(),
                    runtime.tick_ms(),
                    runtime.snapshot_ms(),
                    runtime.autosave_seconds(),
                    runtime.protocol_version(),
                    runtime.save_version(),
                    runtime.map_fingerprint()
                );
                for profile in runtime.environment_profiles() {
                    println!("environment profile {profile:?}");
                }
                println!(
                    "environment profiles truncated={}",
                    runtime.environment_profiles_truncated()
                );
                let impact = runtime.impact();
                println!(
                    "impact active={} craters={} pending={} ready={}",
                    impact.active, impact.craters, impact.pending, impact.ready
                );
                let excavation = runtime.excavation();
                println!(
                    "excavation chunks={} pending={} bytes={} ready={} fluid cells/jobs={}/{} terrain/fluid bytes={}/{} reserved={} failed={}",
                    excavation.chunks(),
                    excavation.pending(),
                    excavation.bytes(),
                    excavation.ready(),
                    excavation.fluid_cells(),
                    excavation.fluid_jobs(),
                    excavation.terrain_payload_bytes(),
                    excavation.fluid_payload_bytes(),
                    excavation.fluid_reserved(),
                    excavation.failed_worlds()
                );
                for percentile in excavation.percentiles() {
                    println!("excavation percentile {percentile:?}");
                }
            }
        }
        CommandPayloadView::Players {
            rows,
            truncated,
            next_cursor,
        } => {
            for row in rows {
                println!(
                    "player {} {} world={} at {:?}",
                    row.id(),
                    row.name(),
                    row.world(),
                    row.position()
                );
            }
            println!("players truncated={truncated} next={next_cursor:?}");
        }
        CommandPayloadView::Metrics {
            snapshot,
            clients,
            clients_truncated,
        } => {
            for field in snapshot.fields() {
                let value = match field.value() {
                    MetricValue::Count(value) => value.to_string(),
                    MetricValue::Optional(value) => format!("{value:?}"),
                    MetricValue::Ratio(value) => value.to_string(),
                    MetricValue::Label(value) => (*value).to_owned(),
                };
                println!("metric {}={value}", field.name());
            }
            for row in clients {
                println!(
                    "traffic {} in={} out={} decoded={} queued={} encoding={} connected={}",
                    row.id(),
                    row.bytes_in(),
                    row.bytes_out(),
                    row.decoded_frames(),
                    row.queued_frames(),
                    row.encoding_us(),
                    row.connected_ms()
                );
            }
            println!("client traffic truncated={clients_truncated}");
        }
        CommandPayloadView::SaveQueued => {
            println!("save queued; status reports pending/completed state");
        }
        CommandPayloadView::Stopping => println!("stopping"),
        CommandPayloadView::Time(clock) | CommandPayloadView::ClockCommitted(clock) => {
            println!(
                "clock id={} nonce={:?} mode={} rate={} offline={} utc={} revision={} fault={} correction={:?} dirty={:?} tick={} epoch={} achieved_ppm={}",
                clock.clock_id(),
                clock.session_nonce(),
                clock.mode(),
                clock.rate_ppm(),
                clock.offline_policy(),
                clock.utc_ms(),
                clock.revision(),
                clock.fault(),
                clock.correction_ms(),
                clock.dirty_revision(), clock.tick(), clock.calendar_epoch(), clock.achieved_ppm()
            );
        }
        CommandPayloadView::Property(report) => {
            println!(
                "property {} requested={:?} effective={:?} default={:?} source={} reason={:?}",
                report.key(),
                report.value().requested,
                report.value().effective,
                report.value().default,
                report.value().source.as_str(),
                report.value().difference_reason
            );
        }
        CommandPayloadView::PropertyHelp(spec) => {
            println!(
                "property {} {:?} default={:?} unit={:?} constraints={:?} owner={} permission={:?} lifecycle={:?} persistent={} resettable={} description={}",
                spec.key,
                spec.value_type,
                spec.default,
                spec.unit,
                spec.constraints,
                spec.owner,
                spec.permission,
                spec.lifecycle,
                spec.persistent,
                spec.resettable,
                spec.description
            );
        }
        CommandPayloadView::PropertyCommandsHelp { builtins } => {
            for command in builtins {
                println!("property command {command}");
            }
        }
        CommandPayloadView::SingleCommandHelp(help) => {
            println!(
                "command help {} args={}",
                help.usage(),
                help.arguments().len()
            );
        }
        CommandPayloadView::PropertyList {
            prefix,
            rows,
            truncated,
        } => {
            for row in rows {
                println!(
                    "property {} {:?} default={:?} unit={:?} constraints={:?} owner={} permission={:?} lifecycle={:?} persistent={} resettable={}",
                    row.key,
                    row.value_type,
                    row.default,
                    row.unit,
                    row.constraints,
                    row.owner,
                    row.permission,
                    row.lifecycle,
                    row.persistent,
                    row.resettable
                );
            }
            println!("property prefix={prefix:?} truncated={truncated}");
        }
        CommandPayloadView::WorldList {
            rows,
            truncated,
            next_cursor,
        } => {
            for row in rows {
                println!(
                    "world {} players={} topology={:?} generation={:?}",
                    row.id(),
                    row.players(),
                    row.topology(),
                    row.generation()
                );
            }
            println!("world list truncated={truncated} next={next_cursor:?}");
        }
        CommandPayloadView::WorldInspect(world) => {
            println!(
                "world {} players={} manifest={:?}",
                world.id(),
                world.players(),
                world.manifest()
            );
        }
        CommandPayloadView::EntityList {
            rows,
            truncated,
            next_cursor,
        } => {
            for row in rows {
                match row.kind() {
                    EntityKindView::Entity(id) => {
                        println!("entity world={} id={id}", row.world());
                    }
                    EntityKindView::Player(id, name) => {
                        println!("entity world={} player={id} name={name}", row.world());
                    }
                }
            }
            println!("entity list truncated={truncated} next={next_cursor:?}");
        }
        CommandPayloadView::EntityInspect {
            world,
            entity,
            rows,
            truncated,
            next_cursor,
        } => {
            println!("entity world={world} id={}", entity.get());
            for row in rows {
                println!(
                    "component {} schema={} bytes={}",
                    row.kind().get(),
                    row.schema().get(),
                    row.bytes()
                );
            }
            println!("entity components truncated={truncated} next={next_cursor:?}");
        }
        CommandPayloadView::EntityDespawn {
            world,
            entity,
            revision,
        } => {
            println!(
                "despawn effect world={world} entity={} revision={revision:?}",
                entity.get()
            );
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
            println!("bolid {id} world={world} observer={} tick={tick} kind={kind} recipients={recipients} shower={shower:?}", observer.get());
        }
        CommandPayloadView::BolidStatus(status) => {
            println!(
                "shower {} state={} emitted={}/{}",
                status.id(),
                status.state().label(),
                status.emitted(),
                status.requested()
            );
        }
        CommandPayloadView::ImpactSpawn {
            id,
            world,
            observer,
            ..
        } => {
            println!(
                "meteorid reserved {id} world={world} observer={}",
                observer.get()
            );
        }
        CommandPayloadView::ImpactReceipt(receipt) => {
            println!(
                "impact {} stage={} journal_revision={:?}",
                receipt.id(),
                receipt.stage().label(),
                receipt.journal_revision()
            );
        }
        CommandPayloadView::WaterStatus(status) => {
            println!(
                "{}",
                minimium_server_runtime::WaterStatusState::label(status.state)
            );
        }
        CommandPayloadView::ModuleText { text, truncated } => {
            println!("module text truncated={truncated}: {text}");
        }
    }
}
