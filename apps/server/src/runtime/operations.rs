use super::*;

impl ServerRuntime {
    pub(super) fn poll_operations(&mut self) -> std::io::Result<Instant> {
        self.operations.persistence.poll_completions();
        let now = Instant::now();
        let monotonic_ms = self.monotonic_ms(now)?;
        if let Some(fault) = self.environment_clock.observe(monotonic_ms) {
            eprintln!("environment clock fault={}", fault.code());
        }
        if now >= self.scheduler.next_utc_check {
            self.scheduler.next_utc_check = now + Duration::from_secs(5);
            match self.environment_clock.check_system_clock(monotonic_ms) {
                Ok(Some(offset_ms)) => eprintln!(
                    "system UTC correction direction={} magnitude_ms={}",
                    if offset_ms >= 0 {
                        "forward"
                    } else {
                        "backward"
                    },
                    offset_ms.abs()
                ),
                Ok(None) => {}
                Err(error) => eprintln!("environment clock correction failed: {error}"),
            }
        }
        self.environment_dirty_revision = self.environment_clock.dirty_revision();
        let autosave_check_at = Instant::now();
        if self
            .scheduler
            .next_autosave
            .is_some_and(|deadline| autosave_check_at >= deadline)
        {
            self.scheduler.next_autosave =
                Some(autosave_check_at + self.scheduler.autosave_interval);
            if !self.operations.persistence.has_pending_save() {
                if let Err(error) = self.operations.persistence.queue_world_save(
                    &mut self.operations.player_store,
                    &self.game.worlds,
                    self.map.fingerprint(),
                ) {
                    eprintln!("could not queue automatic world save: {error}");
                } else {
                    println!("automatic world save queued");
                }
            } else {
                eprintln!("automatic world save skipped because another save is pending");
            }
        }
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
                self.operations
                    .admin
                    .respond(request.id, false, "command queue full");
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
                    eprintln!("ERR command queue full");
                }
            }
        }

        Ok(loop_started)
    }
}
