use crate::executor::Executor;
use crate::{
    connection::{queue_message, Client},
    metrics::RuntimeMetrics,
    world_registry::WorldRegistry,
};
use minimium_core::{ids::PlayerId, world::WorldInstanceId};
use minimium_protocol::{
    RemotePlayer, RemotePlayerIds, RemotePlayers, ServerMessage, MAX_VISIBLE_PLAYERS,
};
use std::{
    collections::BTreeMap,
    time::{Duration, Instant},
};

pub(crate) fn advance_deadline(
    next: &mut Instant,
    interval: Duration,
    metrics: &mut RuntimeMetrics,
) {
    *next += interval;
    let now = Instant::now();
    if now > *next + interval {
        let late_by = now.duration_since(*next).as_nanos();
        metrics.missed_snapshots = metrics
            .missed_snapshots
            .saturating_add((late_by / interval.as_nanos()) as u64);
        *next = now + interval;
    }
}

pub(crate) fn replicate_snapshots(
    worlds: &WorldRegistry,
    executor: &Executor,
    clients: &mut [Client],
    metrics: &mut RuntimeMetrics,
    server_time_ms: u64,
) {
    let mut recipients: BTreeMap<
        WorldInstanceId,
        Vec<(usize, PlayerId, minimium_protocol::WorldContext)>,
    > = BTreeMap::new();
    for (index, client) in clients.iter().enumerate() {
        if client.close_deadline().is_some() {
            continue;
        }
        if let Some(player) = client.player_id() {
            if let (Some(instance), Some(context)) = (
                worlds.instance_for_player(player),
                worlds.context_for_player(player),
            ) {
                recipients
                    .entry(instance)
                    .or_default()
                    .push((index, player, context));
            }
        }
    }
    for (instance, world_recipients) in recipients {
        let Some(world) = worlds.get(instance).map(|owner| &owner.world) else {
            continue;
        };
        let snapshot = world.network_snapshot();
        let snapshots = executor.map(world_recipients, move |(index, recipient, context)| {
            let character_state = snapshot.character_state(recipient).unwrap_or_default();
            let mut players = RemotePlayers::default();
            snapshot.visit_nearby::<MAX_VISIBLE_PLAYERS>(
                recipient,
                MAX_VISIBLE_PLAYERS,
                |player| {
                    let _ = players.push(RemotePlayer {
                        id: player.id,
                        name: player.name.clone(),
                        position: player.position,
                    });
                },
            );
            (index, context, character_state, players)
        });
        for (recipient, context, character_state, current_players) in snapshots {
            if let Some(client) = clients.get_mut(recipient) {
                let full_snapshot = !client.replication_initialized;
                let (players, removed_players) = if full_snapshot {
                    (current_players.clone(), RemotePlayerIds::default())
                } else {
                    diff_remote_players(&client.replicated_players, &current_players)
                };
                let message = ServerMessage::Snapshot {
                    server_time_ms,
                    context,
                    full_snapshot,
                    self_position: character_state.position,
                    self_velocity: character_state.velocity,
                    self_grounded: character_state.grounded,
                    players,
                    removed_players,
                };
                if queue_message(client, metrics, message) {
                    client.replicated_players = current_players;
                    client.replication_initialized = true;
                } else {
                    metrics.dropped_snapshots = metrics.dropped_snapshots.saturating_add(1);
                }
            }
        }
    }
}

pub(crate) fn diff_remote_players(
    previous: &RemotePlayers,
    current: &RemotePlayers,
) -> (RemotePlayers, RemotePlayerIds) {
    let mut changed = RemotePlayers::default();
    for player in current.iter() {
        let differs = previous
            .get(player.id)
            .is_none_or(|old| old.name != player.name || old.position != player.position);
        if differs {
            let _ = changed.push(player.clone());
        }
    }

    let mut removed = RemotePlayerIds::default();
    for previous_player in previous.iter() {
        if current.get(previous_player.id).is_none() {
            let _ = removed.push(previous_player.id);
        }
    }
    (changed, removed)
}
