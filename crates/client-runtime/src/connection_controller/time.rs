use super::*;

impl ConnectionController {
    pub fn request_time_scale(
        &mut self,
        value: String,
        now: ClientTime,
    ) -> Result<(), ControllerError> {
        self.accept_time(now).map_err(ControllerError::from_time)?;
        minimium_environment::parse_scale_ppm(&value).map_err(|message| {
            ControllerError::new("client_runtime.invalid_time_scale", message)
        })?;
        self.request_admin_command(format!("time scale {value}"))
    }

    pub fn request_time_mode(
        &mut self,
        mode: &str,
        now: ClientTime,
    ) -> Result<(), ControllerError> {
        self.accept_time(now).map_err(ControllerError::from_time)?;
        if !matches!(mode, "real_time" | "time_scale") {
            return Err(ControllerError::new(
                "client_runtime.invalid_time_mode",
                "time mode must be real_time or time_scale",
            ));
        }
        self.request_admin_command(format!("time mode {mode}"))
    }

    pub fn request_time_offline_policy(
        &mut self,
        policy: &str,
        now: ClientTime,
    ) -> Result<(), ControllerError> {
        self.accept_time(now).map_err(ControllerError::from_time)?;
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
        now: ClientTime,
    ) -> Result<(), ControllerError> {
        self.accept_time(now).map_err(ControllerError::from_time)?;
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

    pub fn request_random_bolid(
        &mut self,
        count: u16,
        now: ClientTime,
    ) -> Result<(), ControllerError> {
        self.accept_time(now).map_err(ControllerError::from_time)?;
        if !(1..=64).contains(&count) {
            return Err(ControllerError::new(
                "client_runtime.invalid_bolid_count",
                "bolid count must be 1..64",
            ));
        }
        self.request_admin_command(format!("spawn_random_bolid {count}"))
    }

    pub fn request_random_meteorid(&mut self, now: ClientTime) -> Result<(), ControllerError> {
        self.accept_time(now).map_err(ControllerError::from_time)?;
        self.request_admin_command("spawn_random_meteorid".into())
    }
}
