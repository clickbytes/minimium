use super::*;

impl ServerRuntime {
    pub(super) fn monotonic_ms(&self, now: Instant) -> io::Result<MonotonicMs> {
        let elapsed = now.saturating_duration_since(self.scheduler.server_started);
        let millis = u64::try_from(elapsed.as_millis()).map_err(|_| {
            io::Error::new(io::ErrorKind::InvalidData, "server monotonic time overflow")
        })?;
        Ok(MonotonicMs::new(millis))
    }

    pub(super) fn plan_sleep(&self) {
        let poll_interval = if self.connections.clients.is_empty() {
            Duration::from_millis(50)
        } else {
            Duration::from_millis(8)
        };
        let now = Instant::now();
        let until_tick = self.scheduler.next_tick.saturating_duration_since(now);
        let until_snapshot = self.scheduler.next_snapshot.saturating_duration_since(now);
        let until_autosave = self
            .scheduler
            .next_autosave
            .map(|deadline| deadline.saturating_duration_since(now))
            .unwrap_or(Duration::MAX);
        thread::sleep(
            poll_interval
                .min(until_tick)
                .min(until_snapshot)
                .min(until_autosave),
        );
    }
}
