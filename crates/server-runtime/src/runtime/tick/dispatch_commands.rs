use super::*;

impl ServerRuntime {
    pub(in crate::runtime) fn apply_commands(&mut self) {
        let game_address = self.game_address.clone();
        let max_players = self.max_players;
        let lanes = self.lanes;
        let command_phase = allocations::enter(Phase::Commands);
        for _ in 0..8.min(self.operations.command_queue.len()) {
            let Some((request_id, command)) = self.operations.command_queue.pop_front() else {
                break;
            };
            if matches!(request_id, CommandOrigin::Admin(id,_) if {
                !self
                    .operations
                    .transport
                    .is_admin_pending(id, self.now)
            }) {
                continue;
            }
            use crate::server_command::ServerCommand as Cmd;
            let failed = self.excavation.failed() || self.impacts.failed();
            let boundary_command = matches!(
                command,
                Cmd::Time(_)
                    | Cmd::TimeOfDay { .. }
                    | Cmd::Save
                    | Cmd::SpawnRandomMeteorid
                    | Cmd::SpawnRandomBolid(_)
            ) && !matches!(
                command,
                Cmd::Time(minimium_environment::TimeCommand::Status)
            );
            if !failed && self.simulation_work_pending() && boundary_command {
                self.operations
                    .command_queue
                    .push_back((request_id, command));
                continue;
            }
            let mut preparation_error = None;
            if boundary_command && failed {
                preparation_error = Some("authoritative worker failed".to_owned());
            } else if matches!(command, Cmd::Save) {
                match self.prepare_save_boundary() {
                    Ok(false) => {
                        self.operations
                            .command_queue
                            .push_back((request_id, command));
                        continue;
                    }
                    Ok(true) => {}
                    Err(error) => preparation_error = Some(error.to_string()),
                }
            }
            let monotonic_ms = self.monotonic_ms(self.now);
            let schedule_diagnostics = self.schedule.diagnostics();
            let runtime_status = command
                .is_status()
                .then(|| self.runtime_status_snapshot(&game_address, max_players));
            let client_traffic = if command.is_metrics() {
                self.client_traffic_snapshot()
            } else {
                commands::ClientTrafficSnapshot::default()
            };
            let reply = match &command {
                _ if preparation_error.is_some() => commands::CommandReply::error(
                    "server.simulation_failed",
                    preparation_error.as_deref().unwrap(),
                ),
                Cmd::SpawnRandomMeteorid | Cmd::SpawnRandomBolid(_)
                    if self.environment_clock.rate().get() == 0 =>
                {
                    commands::CommandReply::error("server.paused", "simulation paused")
                }
                crate::server_command::ServerCommand::Registered {
                    identity,
                    arguments,
                } => self.execute_module_command(Some(identity), &arguments),
                crate::server_command::ServerCommand::UnresolvedModule(argv) => {
                    self.execute_module_command(None, &argv)
                }
                crate::server_command::ServerCommand::SpawnRandomMeteorid => {
                    self.spawn_meteorid(true)
                }
                crate::server_command::ServerCommand::Property(property) => {
                    self.apply_property_command(property)
                }
                crate::server_command::ServerCommand::SpawnRandomBolid(count) => {
                    self.start_bolid_shower(*count)
                }
                crate::server_command::ServerCommand::BolidStatus(id) => {
                    match self.bolid_shower_status(*id) {
                        Some(status) => commands::CommandReply::success(
                            commands::CommandPayload::BolidStatus(status),
                        ),
                        None => commands::CommandReply::error(
                            "server.bolid.untracked",
                            "shower is unknown or expired",
                        ),
                    }
                }
                crate::server_command::ServerCommand::ImpactReceiptStatus { world, id } => {
                    match self.impacts.impact_receipt(*world, *id) {
                        Some(receipt) => commands::CommandReply::success(
                            commands::CommandPayload::ImpactReceipt(receipt),
                        ),
                        None => commands::CommandReply::error(
                            "server.impact.untracked",
                            "impact receipt is unknown or expired",
                        ),
                    }
                }
                crate::server_command::ServerCommand::WaterStatus(instance) => {
                    match self.water_status_snapshot(*instance) {
                        Some(status) => commands::CommandReply::success(
                            commands::CommandPayload::WaterStatus(status),
                        ),
                        None => commands::CommandReply::error(
                            "server.water.unknown_instance",
                            "world instance not found",
                        ),
                    }
                }
                Cmd::Save => match monotonic_ms {
                    Ok(at) => self.queue_manual_save(at),
                    Err(error) => commands::CommandReply::error(
                        "server.monotonic_clock_failed",
                        &error.to_string(),
                    ),
                },
                Cmd::SaveStatus(id) => {
                    commands::CommandReply::success(commands::CommandPayload::SaveStatus(
                        self.operations.save_jobs.status(*id, self.now),
                    ))
                }
                _ => match monotonic_ms {
                    Ok(monotonic_ms) => commands::execute(
                        &command,
                        commands::OwnerCommandContext {
                            worlds: &self.game.worlds,
                            persistence: &mut *self.operations.persistence,
                            lanes,
                            metrics: &self.metrics,
                            environment_clock: &mut self.environment_clock,
                            monotonic_ms,
                            schedule_diagnostics,
                            runtime_status,
                            client_traffic,
                        },
                        &self.operation_catalog,
                        &self.module_registry,
                    ),
                    Err(error) => commands::CommandReply::error(
                        "server.monotonic_clock_failed",
                        &format!("monotonic clock: {error}"),
                    ),
                },
            };
            self.scheduler
                .demand
                .control_changed(self.environment_clock.rate().get());
            if self.environment_clock.rate().get() == 0 {
                self.service_inputs();
            }
            self.environment_dirty_revision = self.environment_clock.dirty_revision();
            if !reply.is_success() {
                crate::diagnostics::emit(
                    &self.server_logger,
                    minimium_diagnostics::Level::Warn,
                    "server.command_execution_failed",
                    &[(
                        "error_code",
                        reply
                            .error_code()
                            .unwrap_or("server.command.failed")
                            .to_owned(),
                    )],
                );
            }
            if let CommandOrigin::Admin(id, correlation) = request_id {
                if let Err(error) = self.operations.transport.respond_admin(
                    id,
                    crate::admin::response(&reply, correlation, &command, &self.operation_catalog),
                ) {
                    crate::diagnostics::emit(
                        &self.server_logger,
                        minimium_diagnostics::Level::Debug,
                        "server.admin_response_dropped",
                        &[("reason", error.to_string())],
                    );
                }
            } else if matches!(request_id, CommandOrigin::Console) {
                let prefix = match reply.error_code() {
                    Some(code) => format!("ERR {code}"),
                    None => String::from("ERR"),
                };
                let text = crate::command_output::render_reply(&reply);
                println!(
                    "{} {}",
                    if reply.is_success() { "OK" } else { &prefix },
                    text.as_str()
                );
            }
            if command.is_save_or_stop() {
                crate::diagnostics::emit(
                    &self.server_logger,
                    minimium_diagnostics::Level::Info,
                    "server.admin_command_completed",
                    &[
                        (
                            "action",
                            (if matches!(command, crate::server_command::ServerCommand::Stop) {
                                "stop"
                            } else {
                                "save"
                            })
                            .to_owned(),
                        ),
                        ("success", reply.is_success().to_string()),
                    ],
                );
            }
            if reply.should_stop() {
                self.shutdown_request.request();
            }
            if let CommandOrigin::Host(id) = request_id {
                self.operations.host_outcomes.push(crate::CommandExecution {
                    id,
                    identity: command.identity(id.get(), &self.operation_catalog),
                    reply,
                });
            }
            if self.shutdown_request.is_requested() {
                break;
            }
        }
        drop(command_phase);
    }
}
