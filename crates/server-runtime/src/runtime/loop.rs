use super::*;

impl ServerRuntime {
    pub(crate) fn run(mut self) -> io::Result<()> {
        let mut runtime_error = None;
        while !self.shutdown_request.is_requested() {
            let time = crate::RuntimeTime::system_now();
            self.metrics.sample_process_memory(time.monotonic);
            match self.step(time) {
                Ok(outcome) if outcome.stopping => break,
                Ok(outcome) => {
                    std::thread::sleep(outcome.next_wake.saturating_duration_since(Instant::now()))
                }
                Err(error) => {
                    runtime_error = Some(error);
                    break;
                }
            }
        }
        let persistence_deadline = self.shutdown_request.begin_shutdown() - Duration::from_secs(2);
        let time = crate::RuntimeTime::system_now();
        let result = self
            .finish_shutdown(time, persistence_deadline, runtime_error)
            .result;
        if result.is_ok() {
            println!("server stopped cleanly");
        }
        result
    }

    /// Freeze the owner, cancel unexecuted commands and drain explicit services
    /// under the host's absolute monotonic deadline. No new admission is allowed.
    pub fn shutdown(self, time: crate::RuntimeTime, deadline: Instant) -> crate::ShutdownOutcome {
        self.finish_shutdown(time, deadline, None)
    }

    fn finish_shutdown(
        mut self,
        time: crate::RuntimeTime,
        persistence_deadline: Instant,
        mut runtime_error: Option<io::Error>,
    ) -> crate::ShutdownOutcome {
        if time.monotonic < self.now {
            runtime_error.get_or_insert_with(|| {
                io::Error::new(io::ErrorKind::InvalidInput, "shutdown time moved backwards")
            });
        } else {
            self.now = time.monotonic;
            self.environment_clock.set_host_utc(time.utc);
        }
        self.shutdown_request.request();
        self.finish_active_shower(
            super::bolid::receipts::ShowerState::Cancelled,
            Some("server shutdown"),
        );
        self.cancel_commands();
        let commands = std::mem::take(&mut self.operations.host_outcomes);
        let effect_receipts = self.schedule.drain_pending_effect_receipts();
        if !effect_receipts.is_empty() {
            super::diagnostics::emit_system_effect_diagnostic(
                &self.world_logger,
                "effect_receipt:shutdown",
                &format!(
                    "{} unread producer receipts returned by shutdown",
                    effect_receipts.len()
                ),
            );
        }
        self.operations.transport.stop_admission();
        while self.scheduler.pending.is_some() && Instant::now() < persistence_deadline {
            self.poll_impact_journal();
            self.update_excavation();
            if self.finish_pending_step()
                || self.excavation.failed()
                || !self.impact_status_snapshot().ready
            {
                break;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        let mut capture_allowed = self.scheduler.pending.is_none();
        if !capture_allowed {
            runtime_error = Some(io::Error::other("incomplete simulation step at shutdown"));
        }
        if let Err(error) = self.shutdown_excavation(persistence_deadline) {
            capture_allowed = false;
            runtime_error = Some(error);
        }
        if let Err(error) = self.shutdown_impacts(persistence_deadline) {
            capture_allowed = false;
            runtime_error = Some(error);
        }
        self.operations
            .player_store
            .set_excavation_revision(self.excavation.journal_revision());
        let process_lifecycle = self
            .process_lifecycle
            .as_mut()
            .map(|service| &mut **service as &mut dyn crate::composition::ProcessLifecyclePort);
        let result = lifecycle::shutdown(lifecycle::ShutdownContext {
            deadline: persistence_deadline,
            snapshot_at: self.now,
            capture_allowed,
            transport: &mut *self.operations.transport,
            player_store: &mut self.operations.player_store,
            persistence: &mut *self.operations.persistence,
            content: &mut *self.content,
            sector_workers: &mut self.sector_workers,
            executor: &mut self.executor,
            environment_clock: &mut self.environment_clock,
            server_started: self.scheduler.server_started,
            worlds: &self.game.worlds,
            map_fingerprint: self.map.fingerprint(),
            process_lifecycle,
            logger: &self.server_logger,
            runtime_error,
        });
        self.component_owner_authority.revoke_all();
        crate::ShutdownOutcome {
            commands,
            effect_receipts,
            result,
        }
    }
}
