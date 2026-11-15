use super::*;

impl ServerRuntime {
    pub(super) fn poll_operations(&mut self) -> std::io::Result<Instant> {
        self.operations.persistence.poll_completions();
        let now = Instant::now();
        let monotonic_ms = self.monotonic_ms(now)?;
        match self.environment_clock.observe(monotonic_ms) {
            Ok(Some(fault)) => crate::diagnostics::emit(
                &self.environment_logger,
                minimium_diagnostics::Level::Warn,
                "environment.clock_fault",
                &[("fault_code", fault.code().to_owned())],
            ),
            Ok(None) => {}
            Err(error) => crate::diagnostics::emit(
                &self.environment_logger,
                minimium_diagnostics::Level::Error,
                "environment.clock_observation_failed",
                &[
                    ("error_code", error.code().to_owned()),
                    ("reason", error.to_string()),
                ],
            ),
        }
        if now >= self.scheduler.next_utc_check {
            self.scheduler.next_utc_check = now + Duration::from_secs(5);
            match self.environment_clock.check_system_clock(monotonic_ms) {
                Ok(Some(offset_ms)) => crate::diagnostics::emit(
                    &self.environment_logger,
                    minimium_diagnostics::Level::Info,
                    "environment.system_clock_correction",
                    &[
                        (
                            "direction",
                            (if offset_ms >= 0 {
                                "forward"
                            } else {
                                "backward"
                            })
                            .to_owned(),
                        ),
                        ("offset_ms", offset_ms.to_string()),
                    ],
                ),
                Ok(None) => {}
                Err(error) => crate::diagnostics::emit(
                    &self.environment_logger,
                    minimium_diagnostics::Level::Error,
                    "environment.system_clock_correction_failed",
                    &[
                        ("error_code", error.code().to_owned()),
                        ("reason", error.to_string()),
                    ],
                ),
            }
        }
        self.environment_dirty_revision = self.environment_clock.dirty_revision();
        self.poll_autosave()?;
        self.metrics.sample_process_memory(Instant::now());
        self.connections.messages.clear();
        self.connections.disconnected.clear();
        let loop_started = Instant::now();
        let admin_requests = self.operations.admin.poll(Instant::now())?;
        for request in admin_requests {
            if self.operations.command_queue.len() < 64 {
                self.operations
                    .command_queue
                    .push_back((Some(request.id), request.command));
            } else {
                self.operations.admin.respond(
                    request.id,
                    false,
                    Some("server.command.queue_full"),
                    "command queue full",
                );
            }
        }
        if let Some(console_rx) = &self.operations.console_rx {
            for _ in 0..64 {
                let Ok(command) = console_rx.try_recv() else {
                    break;
                };
                if self.operations.command_queue.len() < 64 {
                    self.operations.command_queue.push_back((None, command));
                } else {
                    crate::diagnostics::emit(
                        &self.server_logger,
                        minimium_diagnostics::Level::Warn,
                        "server.command_queue_full",
                        &[],
                    );
                }
            }
        }

        Ok(loop_started)
    }

    fn poll_autosave(&mut self) -> std::io::Result<()> {
        let autosave_check_at = Instant::now();
        if self
            .scheduler
            .next_autosave
            .is_some_and(|deadline| autosave_check_at >= deadline)
        {
            self.scheduler.next_autosave =
                Some(autosave_check_at + self.scheduler.autosave_interval);
            if !self.operations.persistence.has_pending_save() {
                let capture_at = self.monotonic_ms(Instant::now())?;
                let snapshot_clock = self.environment_clock.capture_saved_state(capture_at);
                self.environment_dirty_revision = self.environment_clock.dirty_revision();
                match snapshot_clock {
                    Ok(saved_clock) => {
                        let save_input = self.game.worlds.snapshot_for_save(self.map.fingerprint());
                        let queue_result = save_input
                            .map_err(crate::storage::StorageError::from)
                            .and_then(|input| {
                                self.operations.persistence.queue_world_save(
                                    &mut self.operations.player_store,
                                    input,
                                    saved_clock,
                                )
                            });
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
