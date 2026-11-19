use super::*;
use crate::connection::queue_message;
use minimium_protocol::{ServerMessage, WorldContext};

pub(super) struct Cursor {
    context: WorldContext,
    initial: bool,
    begun: bool,
    initial_ids: std::collections::VecDeque<u64>,
    states: BTreeMap<u64, BodyState>,
}

impl Cursor {
    fn new(context: WorldContext, bodies: &BTreeMap<(WorldInstanceId, u64), BodyState>) -> Self {
        Self {
            context,
            initial: true,
            begun: false,
            states: BTreeMap::new(),
            initial_ids: bodies
                .values()
                .filter(|b| b.world == context.instance())
                .map(|b| b.id)
                .collect(),
        }
    }
}

impl ServerRuntime {
    pub(in crate::runtime) fn replicate_impacts(&mut self) {
        let now = self.environment_clock.boundary().simulation_ms();
        let bodies = &self.impacts.bodies;
        let cursors = &mut self.impacts.clients;
        let clients = &mut self.connections.clients;
        let metrics = &mut self.metrics;
        for client in clients.iter_mut() {
            let Some(player) = client.player_id() else {
                continue;
            };
            let Some(context) = self.game.worlds.context_for_player(player) else {
                continue;
            };
            let cursor = cursors
                .entry(player)
                .or_insert_with(|| Cursor::new(context, bodies));
            if cursor.context != context {
                *cursor = Cursor::new(context, bodies);
            }
            if !cursor.begun {
                if !queue_message(
                    client,
                    metrics,
                    ServerMessage::ImpactSync {
                        context,
                        complete: false,
                    },
                ) {
                    continue;
                }
                cursor.begun = true;
            }
            let mut sent = 0;
            if cursor.initial {
                while let Some(&id) = cursor.initial_ids.front() {
                    if sent == 16 {
                        break;
                    }
                    let Some(&body) = bodies.get(&(context.instance(), id)) else {
                        cursor.initial_ids.pop_front();
                        continue;
                    };
                    if !queue_message(
                        client,
                        metrics,
                        ServerMessage::ImpactState {
                            context,
                            server_time_ms: now,
                            live: false,
                            body,
                        },
                    ) {
                        break;
                    }
                    cursor.states.insert(id, body);
                    cursor.initial_ids.pop_front();
                    sent += 1;
                }
                if !cursor.initial_ids.is_empty() {
                    continue;
                }
            }
            let mut remaining = false;
            for body in bodies.values().filter(|b| b.world == context.instance()) {
                // The bounded initial image has a stable list; moving bodies cannot
                // continually displace old terrain records from its send budget.
                if cursor.initial {
                    break;
                }
                if cursor.states.get(&body.id) == Some(body) {
                    continue;
                }
                if sent >= 16 {
                    remaining = true;
                    break;
                }
                if !queue_message(
                    client,
                    metrics,
                    ServerMessage::ImpactState {
                        context,
                        server_time_ms: now,
                        live: !cursor.initial,
                        body: *body,
                    },
                ) {
                    remaining = true;
                    break;
                }
                cursor.states.insert(body.id, *body);
                sent += 1;
            }
            if cursor.initial
                && !remaining
                && queue_message(
                    client,
                    metrics,
                    ServerMessage::ImpactSync {
                        context,
                        complete: true,
                    },
                )
            {
                cursor.initial = false;
            }
        }
        cursors.retain(|player, _| clients.iter().any(|c| c.player_id() == Some(*player)));
    }
}
