use super::*;

impl ServerRuntime {
    pub(crate) fn run(mut self) -> io::Result<()> {
        let mut runtime_error = None;
        while !self.shutdown_request.is_requested() {
            let loop_started = match self.poll_operations() {
                Ok(started) => started,
                Err(error) => {
                    runtime_error = Some(error);
                    break;
                }
            };
            if self.shutdown_request.is_requested() {
                break;
            }
            if let Err(error) = self.poll_network() {
                runtime_error = Some(error);
                break;
            }
            if self.shutdown_request.is_requested() {
                break;
            }
            self.tick();
            if self.shutdown_request.is_requested() {
                break;
            }
            self.replicate();
            self.metrics.loop_work.record(loop_started.elapsed());
            self.plan_sleep();
        }
        let persistence_deadline = self.shutdown_request.begin_shutdown() - Duration::from_secs(2);
        if let Err(error) = self.shutdown_excavation(persistence_deadline) {
            runtime_error = Some(error);
        }
        if let Err(error) = self.shutdown_impacts(persistence_deadline) {
            runtime_error = Some(error);
        }
        lifecycle::shutdown(lifecycle::ShutdownContext {
            shutdown_request: &self.shutdown_request,
            transport: &mut *self.operations.transport,
            player_store: &mut self.operations.player_store,
            persistence: &mut self.operations.persistence,
            content: &mut self.content,
            sector_workers: &mut self.sector_workers,
            executor: &mut self.executor,
            environment_clock: &mut self.environment_clock,
            server_started: self.scheduler.server_started,
            worlds: &self.game.worlds,
            map_fingerprint: self.map.fingerprint(),
            endpoint: &self.endpoint,
            data_dir: &self.data_dir,
            logger: &self.server_logger,
            runtime_error,
        })
    }
}
