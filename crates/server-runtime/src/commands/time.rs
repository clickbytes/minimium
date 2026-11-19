use super::{CommandPayload, CommandReply};
use crate::environment::EnvironmentClock;
use minimium_environment::time::MonotonicMs;
use minimium_environment::TimeCommand;

pub(super) fn execute(
    command: TimeCommand,
    clock: &mut EnvironmentClock,
    monotonic_ms: MonotonicMs,
) -> CommandReply {
    let result = match command {
        TimeCommand::Status => {
            return match clock.property_snapshot(monotonic_ms) {
                Ok(snapshot) => CommandReply::success(CommandPayload::Time(snapshot)),
                Err(error) => CommandReply::error(error.code(), &error.to_string()),
            }
        }
        TimeCommand::Mode(mode) => clock.set_mode(monotonic_ms, mode),
        TimeCommand::Set(world_utc_ms) => clock.set_date(monotonic_ms, world_utc_ms),
        TimeCommand::Scale(rate_ppm) => clock.set_rate(monotonic_ms, rate_ppm),
        TimeCommand::Offline(policy) => clock.set_offline_policy(monotonic_ms, policy),
    };
    match result {
        Ok(_) => match clock.property_snapshot(monotonic_ms) {
            Ok(snapshot) => CommandReply::success(CommandPayload::ClockCommitted(snapshot)),
            Err(error) => CommandReply::error(error.code(), &error.to_string()),
        },
        Err(error) => CommandReply::error(error.code(), &error.to_string()),
    }
}

pub(super) fn set_day(
    minute: u16,
    solar: bool,
    observer: Option<&(
        minimium_core::ids::PlayerId,
        minimium_commands::results::WorldBinding,
    )>,
    worlds: &crate::world_registry::WorldRegistry,
    clock: &mut EnvironmentClock,
    now: MonotonicMs,
) -> CommandReply {
    let longitude = if solar {
        let Some((actor, expected)) = observer else {
            return CommandReply::error(
                "context_required",
                "solar time requires an actor and expected world context",
            );
        };
        let Some(context) = worlds.context_for_player(*actor).filter(|ctx| {
            ctx.instance().get() == expected.instance
                && ctx.membership_epoch().get() == expected.membership_epoch
                && ctx.content_revision().get() == expected.content_revision
        }) else {
            return CommandReply::error("stale_context", "observer membership has changed");
        };
        let earth = worlds
            .manifest_for_player(*actor)
            .and_then(|m| m.planet())
            .is_some_and(|p| p.mode == 1);
        let Some(position) = worlds
            .get(context.instance())
            .and_then(|instance| instance.world.network_snapshot().position_mm(*actor))
        else {
            return CommandReply::error("stale_context", "observer is no longer in the world");
        };
        match minimium_environment::clock_command::solar_longitude(
            earth,
            position.0.map(|mm| mm as f64 / 1000.0),
        ) {
            Ok(angle) => angle,
            Err(reason) => return CommandReply::error("world.time.solar_unavailable", reason),
        }
    } else {
        0.0
    };
    let instant = match minimium_environment::clock_command::utc_at_time(
        clock.utc().get(),
        minute,
        longitude,
    ) {
        Ok(ms) => minimium_environment::time::WorldTimeMs::new(ms).expect("validated time of day"),
        Err(reason) => return CommandReply::error("world.time.invalid_value", &reason),
    };
    execute(TimeCommand::Set(instant), clock, now)
}
