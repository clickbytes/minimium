use super::*;

impl ConnectionController {
    pub fn cancel(&mut self, now: ClientTime) -> Result<(), ClientTimeError> {
        self.accept_time(now)?;
        self.cancel_current();
        Ok(())
    }

    fn cancel_current(&mut self) {
        if let Some(id) = self.active.take() {
            let _ = self.connector.cancel(id, self.now);
        }
    }

    pub fn finish(&mut self, now: ClientTime) -> Result<(), Box<dyn std::error::Error>> {
        self.finish_until(Instant::now() + Duration::from_secs(28), now)
    }

    pub fn finish_until(
        &mut self,
        deadline: Instant,
        now: ClientTime,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.accept_time(now)?;
        self.finish_resources_until(deadline)
    }

    fn finish_resources_until(
        &mut self,
        deadline: Instant,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.cancel_current();
        let mut unfinished = self.connector.finish_on_exit_until(deadline);
        self.jumps.end_session();
        self.policy.end_session();
        if let Some(mut session) = self.session.take() {
            session.close_resources();
        }
        if let Some(jobs) = self.content_jobs.as_ref() {
            self.content_jobs_shutdown = true;
            if jobs.shutdown_until(deadline) == ContentShutdownStatus::Pending {
                unfinished.push("client-content-jobs".to_owned());
            }
        }
        let cleanup_result = self.cleanup.take().map_or(Ok(()), |worker| {
            wait_for_cleanup(worker, deadline, &mut unfinished)
        });
        let server_result = self.local_server.take().map_or(Ok(()), |mut server| {
            let worker = thread::spawn(move || server.shutdown());
            wait_for_cleanup(worker, deadline, &mut unfinished)
        });
        if let Err(error) = &server_result {
            if cleanup_result.is_err() {
                crate::diagnostics::emit(
                    self.diagnostic_logger.as_ref(),
                    minimium_diagnostics::Level::Error,
                    "client_runtime.local_server_shutdown_failed",
                    [crate::diagnostics::field("reason", error)],
                );
            }
        }
        cleanup_result?;
        if !unfinished.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                format!(
                    "client shutdown exceeded deadline: {}",
                    unfinished.join(",")
                ),
            )
            .into());
        }
        server_result.map_err(Into::into)
    }
}

fn wait_for_cleanup<T: Send + 'static>(
    worker: JoinHandle<io::Result<T>>,
    deadline: Instant,
    unfinished: &mut Vec<String>,
) -> io::Result<T> {
    let name = worker
        .thread()
        .name()
        .unwrap_or("client-cleanup")
        .to_owned();
    let worker = worker;
    while !worker.is_finished() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(5));
    }
    if worker.is_finished() {
        return match worker.join() {
            Ok(result) => result,
            Err(_) => Err(io::Error::other("client cleanup worker panicked")),
        };
    }
    unfinished.push(name);
    drop(worker);
    Err(io::Error::new(
        io::ErrorKind::TimedOut,
        "client cleanup worker exceeded shutdown deadline",
    ))
}

impl Drop for ConnectionController {
    fn drop(&mut self) {
        // An explicit deadline already governed content cleanup. Drop must not
        // silently start another wait; explicit finish_until calls remain retryable.
        if self.content_jobs_shutdown {
            self.content_jobs = None;
        }
        if let Err(error) = self.finish_resources_until(Instant::now() + Duration::from_secs(28)) {
            crate::diagnostics::emit(
                self.diagnostic_logger.as_ref(),
                minimium_diagnostics::Level::Error,
                "client_runtime.local_server_guard_cleanup_failed",
                [crate::diagnostics::field("reason", error)],
            );
        }
    }
}
