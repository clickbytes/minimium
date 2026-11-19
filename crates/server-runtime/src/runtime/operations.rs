use super::*;

impl ServerRuntime {
    pub(super) fn poll_operations(&mut self) -> std::io::Result<Instant> {
        self.operations.persistence.poll_completions();
        self.environment_dirty_revision = self.environment_clock.dirty_revision();
        if !self.simulation_work_pending() {
            self.poll_autosave()?;
        }
        self.connections.messages.clear();
        let loop_started = Instant::now();
        let admin_requests = self
            .connections
            .admin_requests
            .drain(..)
            .collect::<Vec<_>>();
        for (id, line) in admin_requests {
            let command = match crate::admin::parse(&line, &self.operation_catalog) {
                Ok(crate::server_command::ServerCommand::CatalogOutput { text, .. }) => {
                    let reply = crate::commands::CommandReply::module_text(
                        crate::commands::BoundedText::new(&text),
                    );
                    if let Err(error) = self
                        .operations
                        .transport
                        .respond_admin(id, crate::admin::response(&reply))
                    {
                        crate::diagnostics::emit(
                            &self.server_logger,
                            minimium_diagnostics::Level::Debug,
                            "server.admin_response_dropped",
                            &[("reason", error.to_string())],
                        );
                    }
                    continue;
                }
                Ok(command) => command,
                Err(response) => {
                    if let Err(error) = self.operations.transport.respond_admin(id, response) {
                        crate::diagnostics::emit(
                            &self.server_logger,
                            minimium_diagnostics::Level::Debug,
                            "server.admin_response_dropped",
                            &[("reason", error.to_string())],
                        );
                    }
                    continue;
                }
            };
            if self
                .enqueue_command(CommandOrigin::Admin(id), command)
                .is_err()
            {
                let reply = crate::commands::CommandReply::error(
                    "server.command.queue_full",
                    "command queue full",
                );
                if let Err(error) = self
                    .operations
                    .transport
                    .respond_admin(id, crate::admin::response(&reply))
                {
                    crate::diagnostics::emit(
                        &self.server_logger,
                        minimium_diagnostics::Level::Debug,
                        "server.admin_response_dropped",
                        &[("reason", error.to_string())],
                    );
                }
            }
        }
        if let Some(console_rx) = self.operations.console_rx.take() {
            for _ in 0..64 {
                let Ok(command) = console_rx.try_recv() else {
                    break;
                };
                if self
                    .enqueue_command(CommandOrigin::Console, command)
                    .is_err()
                {
                    crate::diagnostics::emit(
                        &self.server_logger,
                        minimium_diagnostics::Level::Warn,
                        "server.command_queue_full",
                        &[],
                    );
                }
            }
            self.operations.console_rx = Some(console_rx);
        }

        Ok(loop_started)
    }

    pub(super) fn prepare_save_boundary(&mut self) -> io::Result<bool> {
        if self.simulation_work_pending() {
            return Ok(false);
        }
        if !self.checkpoint_next_water()? || !self.checkpoint_next_impact()? {
            return Ok(false);
        }
        self.operations
            .player_store
            .set_excavation_revision(self.excavation.journal_revision());
        Ok(true)
    }

    fn poll_autosave(&mut self) -> std::io::Result<()> {
        let autosave_check_at = self.now;
        if self
            .scheduler
            .next_autosave
            .is_some_and(|deadline| autosave_check_at >= deadline)
        {
            if !self.operations.persistence.has_pending_save() && !self.prepare_save_boundary()? {
                return Ok(());
            }
            self.scheduler.next_autosave =
                Some(autosave_check_at + self.scheduler.autosave_interval);
            if !self.operations.persistence.has_pending_save() {
                let capture_at = self.monotonic_ms(self.now)?;
                let snapshot_clock = self.environment_clock.capture_saved_state(capture_at);
                self.environment_dirty_revision = self.environment_clock.dirty_revision();
                match snapshot_clock {
                    Ok(saved_clock) => {
                        let save_input = self.game.worlds.snapshot_for_save(self.map.fingerprint());
                        let queue_result = save_input
                            .map_err(crate::storage::StorageError::from)
                            .and_then(|input| {
                                self.operations
                                    .player_store
                                    .save_snapshot(input, saved_clock)
                            })
                            .and_then(|snapshot| self.operations.persistence.queue_save(snapshot));
                        if let Err(error) = queue_result {
                            crate::diagnostics::emit(
                                &self.server_logger,
                                minimium_diagnostics::Level::Error,
                                "server.autosave_queue_failed",
                                &[("error_code", error.code().to_owned())],
                            );
                        }
                    }
                    Err(error) => {
                        crate::diagnostics::emit(
                            &self.server_logger,
                            minimium_diagnostics::Level::Error,
                            "server.autosave_clock_capture_failed",
                            &[
                                ("error_code", error.code().to_owned()),
                                ("reason", error.to_string()),
                            ],
                        );
                    }
                }
            } else {
                crate::diagnostics::emit(
                    &self.server_logger,
                    minimium_diagnostics::Level::Info,
                    "server.autosave_skipped_pending",
                    &[],
                );
            }
        }
        Ok(())
    }
}
