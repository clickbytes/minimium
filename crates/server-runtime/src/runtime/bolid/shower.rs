//! One active shower and a bounded, ten-minute monotonic terminal ledger.
use super::{
    mix,
    receipts::{BolidShowerStatus, ShowerState},
    CommandReply, Duration, ServerRuntime,
};
use crate::commands::CommandPayload;
use minimium_core::ids::PlayerId;
use minimium_protocol::WorldContext;

pub(crate) struct Shower {
    pub observer: Option<(PlayerId, WorldContext)>,
    status: Option<BolidShowerStatus>,
    remaining: u16,
    next_at: Duration,
    expires_at: Duration,
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
        if !(1..=64).contains(&count) {
            return CommandReply::error("server.bolid.invalid_count", "bolid count must be 1..64");
        }
        if self.bolid_shower.is_some() {
            return CommandReply::error(
                "server.bolid.shower_busy",
                "a bolid shower is already running",
            );
        }
        let now = Duration::from_millis(self.step_boundary().simulation_ms());
        if count > 1 {
            self.bolid_shower = Some(Shower {
                observer: None,
                status: None,
                remaining: count - 1,
                next_at: now + delay(self.next_bolid_id ^ self.step_boundary().tick()),
                expires_at: now + Duration::from_secs(600),
            });
        }
        let mut reply = self.spawn_bolid(true, false);
        if !reply.is_success() {
            self.bolid_shower = None;
            return reply;
        }
        let Some(CommandPayload::Bolid {
            id,
            world,
            observer,
            context,
            tick,
            ..
        }) = reply.payload()
        else {
            unreachable!("successful manual bolid has an event receipt")
        };
        let mut status = BolidShowerStatus {
            id: *id,
            requested: count,
            event_ids: vec![*id],
            world: *world,
            observer: *observer,
            context: *context,
            state: ShowerState::Running,
            reason: None,
            started_tick: *tick,
            terminal_tick: None,
            terminal_at: None,
        };
        if count == 1 {
            status.finish(ShowerState::Completed, None, *tick, self.now);
            self.retain_shower(status);
        } else {
            self.bolid_shower.as_mut().expect("active shower").status = Some(status);
            if let Some(CommandPayload::Bolid { shower, .. }) = reply.payload_mut() {
                *shower = Some(count as usize);
            }
        }
        reply
    }

    pub(in crate::runtime) fn update_bolid_shower(&mut self, now: Duration) {
        self.expire_shower_history();
        let Some(shower) = self.bolid_shower.as_ref() else {
            return;
        };
        if now >= shower.expires_at {
            self.finish_active_shower(ShowerState::PartiallyCompleted, Some("simulation expiry"));
            return;
        }
        if shower
            .observer
            .is_some_and(|(player, context)| !self.game.worlds.context_matches(player, context))
        {
            self.finish_active_shower(
                ShowerState::PartiallyCompleted,
                Some("observer world context changed"),
            );
            return;
        }
        if now < shower.next_at {
            return;
        }
        // At most one event per tick; a stalled server never emits a catch-up burst.
        let reply = self.spawn_bolid(true, true);
        if !reply.is_success() {
            let reason = reply.detail().unwrap_or("bolid event rejected").to_owned();
            self.finish_active_shower(ShowerState::PartiallyCompleted, Some(&reason));
            crate::diagnostics::emit(
                &self.world_logger,
                minimium_diagnostics::Level::Warn,
                "bolid.shower_cancelled",
                &[("reason", reason)],
            );
            return;
        }
        let Some(CommandPayload::Bolid { id, .. }) = reply.payload() else {
            unreachable!("successful bolid event receipt")
        };
        let tick = self.step_boundary().tick();
        let shower = self.bolid_shower.as_mut().expect("active shower");
        let status = shower.status.as_mut().expect("first event recorded");
        status.event_ids.push(*id);
        shower.remaining -= 1;
        if shower.remaining == 0 {
            self.finish_active_shower(ShowerState::Completed, None);
        } else {
            shower.next_at = now + delay(self.next_bolid_id ^ tick);
        }
    }

    pub(in crate::runtime) fn bolid_shower_status(&mut self, id: u64) -> Option<BolidShowerStatus> {
        self.expire_shower_history();
        self.bolid_shower
            .as_ref()
            .and_then(|active| active.status.as_ref())
            .filter(|status| status.id == id)
            .or_else(|| self.bolid_history.iter().find(|status| status.id == id))
            .cloned()
    }

    pub(in crate::runtime) fn finish_active_shower(
        &mut self,
        state: ShowerState,
        reason: Option<&str>,
    ) {
        if let Some(mut status) = self.bolid_shower.take().and_then(|active| active.status) {
            status.finish(state, reason, self.step_boundary().tick(), self.now);
            self.retain_shower(status);
        }
    }

    fn retain_shower(&mut self, status: BolidShowerStatus) {
        self.expire_shower_history();
        if self.bolid_history.len() == 64 {
            self.bolid_history.pop_front();
        }
        self.bolid_history.push_back(status);
    }

    fn expire_shower_history(&mut self) {
        while self.bolid_history.front().is_some_and(|status| {
            status.terminal_at.is_some_and(|at| {
                self.now.saturating_duration_since(at) >= Duration::from_secs(600)
            })
        }) {
            self.bolid_history.pop_front();
        }
    }
}
