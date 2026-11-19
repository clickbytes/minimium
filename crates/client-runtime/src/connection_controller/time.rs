use super::*;
use minimium_environment::{
    clock_command::ClockWrite,
    time::{ClockMode, OfflinePolicy},
};

impl ConnectionController {
    pub fn request_clock(
        &mut self,
        change: ClockWrite,
        now: ClientTime,
    ) -> Result<minimium_commands::admin::Correlation, ControllerError> {
        self.accept_time(now).map_err(ControllerError::from_time)?;
        if matches!(change, ClockWrite::HourMinute { minute: 1440.., .. }) {
            return Err(ControllerError::new(
                "client_runtime.invalid_time_request",
                "invalid time of day",
            ));
        }
        self.submit_contextual_admin(
            change.payload(),
            matches!(change, ClockWrite::HourMinute { solar: true, .. }),
        )
    }
    pub fn request_time_scale(
        &mut self,
        value: String,
        now: ClientTime,
    ) -> Result<(), ControllerError> {
        let scale = minimium_environment::parse_scale_ppm(&value)
            .map_err(|reason| ControllerError::new("client_runtime.invalid_time_scale", reason))?;
        self.request_clock(ClockWrite::Scale(scale), now)
            .map(|_| ())
    }
    pub fn request_time_mode(
        &mut self,
        mode: &str,
        now: ClientTime,
    ) -> Result<(), ControllerError> {
        let mode = match mode {
            "real_time" => ClockMode::RealTime,
            "time_scale" => ClockMode::TimeScale,
            _ => {
                return Err(ControllerError::new(
                    "client_runtime.invalid_time_mode",
                    "unknown clock mode",
                ))
            }
        };
        self.request_clock(ClockWrite::Mode(mode), now).map(|_| ())
    }
    pub fn request_time_offline_policy(
        &mut self,
        policy: &str,
        now: ClientTime,
    ) -> Result<(), ControllerError> {
        let policy = match policy {
            "advance" => OfflinePolicy::Advance,
            "pause" => OfflinePolicy::Pause,
            _ => {
                return Err(ControllerError::new(
                    "client_runtime.invalid_offline_policy",
                    "unknown offline policy",
                ))
            }
        };
        self.request_clock(ClockWrite::Offline(policy), now)
            .map(|_| ())
    }

    /// Compatibility entry point. Zero selects UTC. A solar request uses the server's observer.
    pub fn request_set_time(
        &mut self,
        minute: u16,
        longitude_rad: f64,
        now: ClientTime,
    ) -> Result<(), ControllerError> {
        if !longitude_rad.is_finite() || longitude_rad.abs() > std::f64::consts::PI {
            return Err(ControllerError::new(
                "client_runtime.invalid_time_request",
                "invalid longitude",
            ));
        }
        self.request_clock(
            ClockWrite::HourMinute {
                minute,
                solar: longitude_rad != 0.0,
            },
            now,
        )
        .map(|_| ())
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
