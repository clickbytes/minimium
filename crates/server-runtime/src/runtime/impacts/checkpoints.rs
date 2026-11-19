use super::*;

impl ServerRuntime {
    pub(in crate::runtime) fn poll_impact_journal(&mut self) {
        while let Some(result) = self.impacts.journal.poll() {
            match result {
                Ok(batch) => {
                    if !batch.is_empty()
                        && batch
                            .iter()
                            .all(|body| self.impact_projection_matches(*body))
                    {
                        continue;
                    }
                    if !self.impacts.checkpoints_match(&batch) {
                        self.impact_failure(io::Error::other(
                            "stale impact journal checkpoint batch acknowledgement",
                        ));
                        self.shutdown_request.request();
                        return;
                    }
                    for body in batch {
                        if let Err(error) = self.publish_impact_projection(body, false) {
                            self.impact_failure(error);
                            return;
                        }
                        if let Err(error) = self.impacts.acknowledge_checkpoint(body) {
                            self.impact_failure(error);
                            self.shutdown_request.request();
                            return;
                        }
                        self.impacts.record_impact_committed(
                            body.world,
                            body.id,
                            self.step_boundary().tick(),
                        );
                    }
                }
                Err(error) => {
                    self.impact_failure(error);
                    return;
                }
            }
        }
    }

    pub(in crate::runtime) fn checkpoint_next_impact(&mut self) -> io::Result<bool> {
        if let Some(error) = &self.impacts.failure {
            return Err(io::Error::other(error.clone()));
        }
        if self.impacts.pending_count() != 0 {
            return Ok(false);
        }
        let next = self
            .impacts
            .bodies()
            .copied()
            .find(|body| !self.impact_projection_matches(*body));
        if let Some(body) = next {
            self.queue_body(body)?;
            return Ok(false);
        }
        Ok(true)
    }

    pub(in crate::runtime) fn shutdown_impacts(&mut self, deadline: Instant) -> io::Result<()> {
        let drained = loop {
            self.poll_impact_journal();
            if Instant::now() >= deadline {
                break Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "impact checkpoint deadline",
                ));
            }
            match self.checkpoint_next_impact() {
                Ok(true) => break Ok(()),
                Ok(false) => std::thread::sleep(Duration::from_millis(1)),
                Err(error) => break Err(error),
            }
        };
        let closed = self.impacts.journal.shutdown_until(deadline);
        drained.and(closed)
    }
}
