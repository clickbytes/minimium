use super::ClientApplicationCore;
use crate::{ClientTime, ClientTimeError, ControllerError};

impl ClientApplicationCore {
    /// Checks every participating mutable owner before a host operation starts.
    pub fn validate_time(&self, now: ClientTime) -> Result<(), ClientTimeError> {
        now.validate_after(self.last_operation_at)?;
        self.action_executor.validate_time(now)?;
        self.connection.validate_time(now)?;
        self.game.gameplay.reconciliation.validate_time(now)
    }

    pub(crate) fn accept_time(&mut self, now: ClientTime) -> Result<(), ClientTimeError> {
        self.validate_time(now)?;
        self.last_operation_at = now;
        Ok(())
    }

    pub(crate) fn accept_controller_time(
        &mut self,
        now: ClientTime,
    ) -> Result<(), ControllerError> {
        self.accept_time(now).map_err(|error| {
            ControllerError::caused_by(
                error.code(),
                error.to_string(),
                std::io::Error::other(error),
            )
        })
    }
}
