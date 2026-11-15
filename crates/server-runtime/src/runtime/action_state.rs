use super::GameState;
use crate::world_inputs::WorldInput;
use minimium_core::ids::PlayerId;
use minimium_protocol::{is_unit_direction, valid_chat};
use std::time::Instant;

const CHAT_COOLDOWN: std::time::Duration = std::time::Duration::from_secs(1);
const GLOBAL_CHAT_COOLDOWN: std::time::Duration = std::time::Duration::from_millis(50);

#[derive(Default)]
pub(super) struct ActorActionCooldowns {
    last_fire_at: Option<Instant>,
    last_chat_at: Option<Instant>,
}

pub(super) enum ActorAction {
    Fire([f32; 3]),
    Chat(String),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ActorActionAdmission {
    Queued,
    InvalidIntent,
    RateLimited,
    Busy,
    ActorUnavailable,
}

impl GameState {
    pub(super) fn activate_actor_actions(&mut self, player: PlayerId) -> bool {
        if self.worlds.instance_for_player(player).is_none() {
            return false;
        }
        self.actor_action_cooldowns.entry(player).or_default();
        true
    }

    pub(super) fn remove_actor_actions(&mut self, player: PlayerId) {
        self.actor_action_cooldowns.remove(&player);
    }

    /// Applies actor-owned cooldowns and enqueues Fire/Chat atomically. Queue
    /// availability is measured by admission so transport reservations remain
    /// outside the action owner; cooldown is consumed only after enqueue.
    pub(super) fn admit_actor_action(
        &mut self,
        player: PlayerId,
        action: ActorAction,
        decoded_at: Instant,
        now: Instant,
        reserved_transport_inputs: usize,
    ) -> ActorActionAdmission {
        let valid = match &action {
            ActorAction::Fire(direction) => is_unit_direction(*direction),
            ActorAction::Chat(text) => valid_chat(text),
        };
        if !valid {
            return ActorActionAdmission::InvalidIntent;
        }
        let Some(cooldowns) = self.actor_action_cooldowns.get(&player) else {
            return ActorActionAdmission::ActorUnavailable;
        };
        if self.worlds.instance_for_player(player).is_none() {
            return ActorActionAdmission::ActorUnavailable;
        }
        let rate_limited = match &action {
            ActorAction::Fire(_) => cooldowns
                .last_fire_at
                .is_some_and(|last| now.saturating_duration_since(last) < self.fire_cooldown),
            ActorAction::Chat(_) => {
                cooldowns
                    .last_chat_at
                    .is_some_and(|last| now.saturating_duration_since(last) < CHAT_COOLDOWN)
                    || self.last_global_chat_at.is_some_and(|last| {
                        now.saturating_duration_since(last) < GLOBAL_CHAT_COOLDOWN
                    })
            }
        };
        if rate_limited {
            return ActorActionAdmission::RateLimited;
        }
        if self
            .pending_world_inputs
            .len()
            .saturating_add(reserved_transport_inputs)
            >= crate::MAX_PENDING_WORLD_INPUTS
        {
            return ActorActionAdmission::Busy;
        }

        let is_chat = matches!(&action, ActorAction::Chat(_));
        let input = match action {
            ActorAction::Fire(direction) => WorldInput::Fire(player, direction),
            ActorAction::Chat(text) => WorldInput::Chat(player, text),
        };
        let Some(cooldowns) = self.actor_action_cooldowns.get_mut(&player) else {
            return ActorActionAdmission::ActorUnavailable;
        };
        self.pending_world_inputs.push((input, Some(decoded_at)));
        if is_chat {
            cooldowns.last_chat_at = Some(now);
            self.last_global_chat_at = Some(now);
        } else {
            cooldowns.last_fire_at = Some(now);
        }
        ActorActionAdmission::Queued
    }
}
