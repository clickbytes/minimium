use super::CommandReply;
use crate::environment::EnvironmentClock;
use minimium_core::time::MonotonicMs;
use minimium_platform::commands::TimeCommand;

pub(super) fn execute(
    command: TimeCommand,
    clock: &mut EnvironmentClock,
    monotonic_ms: MonotonicMs,
) -> CommandReply {
    let result = match command {
        TimeCommand::Status => {
            return CommandReply {
                success: true,
                text: clock.status(monotonic_ms),
                stop: false,
            };
        }
        TimeCommand::Mode(mode) => clock.set_mode(monotonic_ms, mode),
        TimeCommand::Set(world_utc_ms) => clock.set_date(monotonic_ms, world_utc_ms),
        TimeCommand::Scale(rate_ppm) => clock.set_rate(monotonic_ms, rate_ppm),
        TimeCommand::Offline(policy) => clock.set_offline_policy(monotonic_ms, policy),
    };
    match result {
        Ok(_) => CommandReply {
            success: true,
            text: clock.status(monotonic_ms),
            stop: false,
        },
        Err(error) => CommandReply {
            success: false,
            text: error,
            stop: false,
        },
    }
}
