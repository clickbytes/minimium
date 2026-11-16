use super::*;

impl ClientClock {
    pub(in crate::session) fn reset(&mut self, now: ClientTime) {
        let diagnostic_logger = self.diagnostic_logger.clone();
        *self = Self::new(now, diagnostic_logger);
    }

    pub(in crate::session) fn local_monotonic_ms(&self) -> u64 {
        self.now_ms
    }

    pub(in crate::session) fn validate_time(&self, now: ClientTime) -> Result<(), ClientTimeError> {
        let millis = u64::try_from(now.checked_duration_since(self.epoch)?.as_millis())
            .map_err(|_| ClientTimeError::Overflow)?;
        millis
            .checked_add(300_000)
            .ok_or(ClientTimeError::Overflow)?;
        Ok(())
    }

    pub(in crate::session) fn set_time(&mut self, now: ClientTime) -> Result<(), ClientTimeError> {
        self.validate_time(now)?;
        self.now_ms = u64::try_from(now.checked_duration_since(self.epoch)?.as_millis())
            .map_err(|_| ClientTimeError::Overflow)?;
        Ok(())
    }
}
