use super::*;

impl ConnectionController {
    pub fn request_time_scale(&mut self, value: String) -> Result<(), ControllerError> {
        minimium_environment::parse_scale_ppm(&value).map_err(|message| {
            ControllerError::new("client_runtime.invalid_time_scale", message)
        })?;
        self.request_admin_command(format!("time scale {value}"))
    }

    pub fn request_time_mode(&mut self, mode: &str) -> Result<(), ControllerError> {
        if !matches!(mode, "real_time" | "time_scale") {
            return Err(ControllerError::new(
                "client_runtime.invalid_time_mode",
                "time mode must be real_time or time_scale",
            ));
        }
        self.request_admin_command(format!("time mode {mode}"))
    }

    pub fn request_time_offline_policy(&mut self, policy: &str) -> Result<(), ControllerError> {
        if !matches!(policy, "pause" | "advance") {
            return Err(ControllerError::new(
                "client_runtime.invalid_offline_policy",
                "offline policy must be pause or advance",
            ));
        }
        self.request_admin_command(format!("time offline {policy}"))
    }

    pub fn request_set_time(
        &mut self,
        minute: u16,
        longitude_rad: f64,
    ) -> Result<(), ControllerError> {
        let current = self
            .session
            .as_ref()
            .and_then(|session| session.environment_frame().estimated_world_utc_ms)
            .ok_or_else(|| {
                ControllerError::new(
                    "client_runtime.world_time_unavailable",
                    "synchronized world time is unavailable",
                )
            })?;
        let target =
            minimium_environment::clock_command::utc_at_time(current, minute, longitude_rad)
                .map_err(|message| {
                    ControllerError::new("client_runtime.invalid_time_request", message)
                })?;
        let date = crate::environment::format_utc(target)
            .replace(" UTC", "Z")
            .replace(' ', "T");
        self.request_admin_command(format!("time set {date}"))
    }

    pub fn request_random_bolid(&mut self) -> Result<(), ControllerError> {
        self.request_admin_command("spawn_random_bolid".to_owned())
    }

    fn request_admin_command(&mut self, command: String) -> Result<(), ControllerError> {
        if !self
            .session
            .as_ref()
            .is_some_and(|session| session.state() == SessionState::Connected)
            || !self.session_owns_server
        {
            return Err(ControllerError::new(
                "client_runtime.admin_control_unavailable",
                "administrative commands require a connection to a server started by this client",
            ));
        }
        if self.admin_request.is_some() {
            return Err(ControllerError::new(
                "client_runtime.admin_request_busy",
                "an administrative request is already in progress",
            ));
        }
        let endpoint = self
            .local_server
            .as_mut()
            .ok_or_else(|| {
                ControllerError::new(
                    "client_runtime.local_server_unavailable",
                    "owned server is unavailable",
                )
            })?
            .owned_admin_endpoint()
            .map_err(|error| {
                ControllerError::caused_by(
                    "client_runtime.admin_endpoint_failed",
                    "could not resolve the owned server admin endpoint",
                    error,
                )
            })?
            .ok_or_else(|| {
                ControllerError::new(
                    "client_runtime.local_server_not_running",
                    "owned server is not running",
                )
            })?;
        self.admin_request = Some(thread::spawn(move || {
            admin_client::request(&endpoint, &command).map_err(|error| {
                ControllerError::caused_by(
                    "client_runtime.admin_request_failed",
                    "administrative request failed",
                    error,
                )
            })
        }));
        Ok(())
    }

    pub fn poll_admin_result(&mut self) -> Option<Result<String, ControllerError>> {
        if !self
            .admin_request
            .as_ref()
            .is_some_and(JoinHandle::is_finished)
        {
            return None;
        }
        let worker = self.admin_request.take()?;
        let result = match worker.join() {
            Ok(Ok(response)) => Ok(response),
            Ok(Err(error)) => Err(error),
            Err(_) => Err(ControllerError::new(
                "client_runtime.admin_worker_stopped",
                "administrative request worker panicked",
            )),
        };
        if let Err(error) = &result {
            self.record_diagnostic_failure("client_runtime.controller_failed", &error.to_string());
        }
        Some(result)
    }
}
