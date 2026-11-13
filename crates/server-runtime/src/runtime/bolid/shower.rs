//! One bounded, transient administrative shower; no timer thread or saved state.
use super::{mix, CommandReply, Duration, Instant, ServerRuntime};
use minimium_core::ids::PlayerId;
use minimium_protocol::WorldContext;

pub(crate) struct Shower {
    pub observer: Option<(PlayerId, WorldContext)>,
    remaining: u16,
    next_at: Instant,
    expires_at: Instant,
}

fn delay(seed: u64) -> Duration {
    // Short clusters alternate with longer gaps; no periodic interval.
    let word = mix(seed);
    let milliseconds = if word.is_multiple_of(3) {
        900 + word % 1300
    } else {
        2500 + word % 5500
    };
    Duration::from_millis(milliseconds)
}

impl ServerRuntime {
    pub(in crate::runtime) fn start_bolid_shower(&mut self, count: u16) -> CommandReply {
        let error = |text: &str, code| CommandReply {
            success: false,
            text: text.into(),
            stop: false,
            error_code: Some(code),
        };
        if !(1..=64).contains(&count) {
            return error("bolid count must be 1..64", "server.bolid.invalid_count");
        }
        if self.bolid_shower.is_some() {
            return error(
                "a bolid shower is already running",
                "server.bolid.shower_busy",
            );
        }
        let now = Instant::now();
        if count > 1 {
            self.bolid_shower = Some(Shower {
                observer: None,
                remaining: count - 1,
                next_at: now + delay(self.next_bolid_id ^ self.scheduler.simulation_tick),
                expires_at: now + Duration::from_secs(600),
            });
        }
        let mut reply = self.spawn_bolid(true, false);
        if !reply.success {
            self.bolid_shower = None;
        } else if count > 1 {
            reply.text.push_str(&format!("; shower accepted: {count} bolids total, first now, remaining {} at irregular 0.9..8 s intervals", count - 1));
        }
        reply
    }

    pub(in crate::runtime) fn update_bolid_shower(&mut self, now: Instant) {
        let Some(shower) = self.bolid_shower.as_ref() else {
            return;
        };
        if now >= shower.expires_at {
            self.bolid_shower = None;
            return;
        }
        if now < shower.next_at {
            return;
        }
        // At most one event per tick; a stalled server never emits a catch-up burst.
        let reply = self.spawn_bolid(true, true);
        if !reply.success {
            self.bolid_shower = None;
            crate::diagnostics::emit(
                &self.world_logger,
                minimium_diagnostics::Level::Warn,
                "bolid.shower_cancelled",
                &[("reason", reply.text)],
            );
            return;
        }
        if let Some(shower) = &mut self.bolid_shower {
            shower.remaining -= 1;
            if shower.remaining == 0 {
                self.bolid_shower = None;
            } else {
                shower.next_at = now + delay(self.next_bolid_id ^ self.scheduler.simulation_tick);
            }
        }
    }
}
