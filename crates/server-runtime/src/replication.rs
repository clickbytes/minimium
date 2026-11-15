mod geometry;
mod player_diff;
use geometry::entity_sector_for_position;
pub(crate) use player_diff::diff_remote_players;

use crate::executor::Executor;
use crate::{
    connection::{queue_encoded_frame_with_reserve, queue_message, Client},
    metrics::RuntimeMetrics,
    world_registry::WorldRegistry,
};
use minimium_core::ids::PlayerId;
use minimium_protocol::{
    EntityReplicationOperation, RemotePlayer, RemotePlayerIds, RemotePlayers, ServerMessage,
    MAX_SERVER_FRAME_BYTES, MAX_VISIBLE_PLAYERS,
};
use minimium_world::entity::{
    ComponentMutation, ComponentRegistry, ReplicationPolicy, ReplicationVisibility,
};
use minimium_world::world::{SectorKey, WorldInstanceId, WorldManifest, WorldTopology};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap, VecDeque},
    time::{Duration, Instant},
};

const ENTITY_SYNC_BUDGET_PER_TICK: usize = 64 * 1024;
const ENTITY_SYNC_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_REPLICATED_ENTITIES: usize = 256;

pub(crate) fn replicate_entities(
    worlds: &WorldRegistry,
    registry: &ComponentRegistry,
    tickets: &HashMap<PlayerId, Vec<minimium_sectors::SectorTicket>>,
    clients: &mut [Client],
    metrics: &mut RuntimeMetrics,
) {
    let now = Instant::now();
    for client in clients.iter_mut() {
        let Some(player) = client.player_id() else {
            continue;
        };
        let (Some(instance), Some(context)) = (
            worlds.instance_for_player(player),
            worlds.context_for_player(player),
        ) else {
            continue;
        };
        let Some(owner) = worlds.get(instance) else {
            continue;
        };
        if client
            .entity_sync
            .as_ref()
            .is_some_and(|sync| sync.context != context)
        {
            client.entity_sync = None;
            client.entity_revision = 0;
            client.entity_sync_initialized = false;
            client.replicated_components.clear();
        }
        if client
            .entity_sync
            .as_ref()
            .is_some_and(|sync| now.duration_since(sync.started) >= ENTITY_SYNC_TIMEOUT)
        {
            client.entity_sync = None;
            client.entity_revision = 0;
            client.entity_sync_initialized = false;
            client.replicated_components.clear();
        }
        if client.entity_sync.is_none() {
            let active: BTreeSet<_> = tickets
                .get(&player)
                .into_iter()
                .flatten()
                .filter(|ticket| ticket.instance() == instance)
                .map(|ticket| ticket.key())
                .collect();
            let player_position = owner
                .world
                .snapshot()
                .into_iter()
                .find(|snapshot| snapshot.id == player)
                .map(|snapshot| snapshot.position);
            let mut component_views: std::collections::BTreeMap<_, Vec<_>> =
                std::collections::BTreeMap::new();
            for (entity, kind, blob) in owner.entities.iter_components() {
                let Some(descriptor) = registry
                    .descriptor(kind)
                    .filter(|descriptor| descriptor.replication == ReplicationPolicy::Replicated)
                else {
                    continue;
                };
                component_views.entry(entity).or_default().push((
                    kind,
                    blob,
                    descriptor.visibility,
                ));
            }
            let mut candidates = Vec::new();
            for (entity, components) in &component_views {
                let mut nearest = f64::INFINITY;
                let mut visible_components = Vec::new();
                for (kind, blob, visibility) in components {
                    match visibility {
                        ReplicationVisibility::Global => visible_components.push((*kind, *blob)),
                        ReplicationVisibility::Spatial { position } => {
                            let Some(position) = position(&blob.bytes) else {
                                continue;
                            };
                            let Some(sector) = entity_sector_for_position(owner.manifest, position)
                            else {
                                continue;
                            };
                            if !active.contains(&sector) {
                                continue;
                            }
                            visible_components.push((*kind, *blob));
                            if let Some(player_position) = player_position {
                                let distance = position
                                    .iter()
                                    .zip(player_position)
                                    .map(|(entity, player)| {
                                        (f64::from(*entity) - f64::from(player)).powi(2)
                                    })
                                    .sum::<f64>();
                                nearest = nearest.min(distance);
                            }
                        }
                    }
                }
                if !visible_components.is_empty() {
                    candidates.push((nearest, *entity, visible_components));
                }
            }
            candidates.sort_by(
                |(left_distance, left_id, _), (right_distance, right_id, _)| {
                    left_distance
                        .total_cmp(right_distance)
                        .then_with(|| left_id.cmp(right_id))
                },
            );
            let filtered = candidates.len().saturating_sub(MAX_REPLICATED_ENTITIES);
            metrics.record_component_candidates(candidates.len() - filtered, filtered);
            candidates.truncate(MAX_REPLICATED_ENTITIES);
            let visible: Vec<_> = candidates
                .into_iter()
                .map(|(_, entity, components)| (entity, components))
                .collect();
            let visible_components: std::collections::BTreeMap<_, BTreeSet<_>> = visible
                .iter()
                .map(|(entity, components)| {
                    (*entity, components.iter().map(|(kind, _)| *kind).collect())
                })
                .collect();
            let mut entity_records = std::collections::BTreeMap::new();
            for (entity, components) in &visible {
                let mut records = vec![EntityReplicationOperation::Spawn { entity: *entity }];
                records.extend(components.iter().map(|(kind, blob)| {
                    EntityReplicationOperation::ComponentUpsert {
                        entity: *entity,
                        kind: *kind,
                        schema: blob.schema,
                        bytes: blob.bytes.clone(),
                    }
                }));
                entity_records.insert(*entity, records);
            }
            let target_revision = owner.entities.revision();
            if client.entity_sync_initialized
                && client.entity_revision == target_revision
                && client.replicated_components == visible_components
            {
                continue;
            }
            let journal_floor = owner.entities.oldest_change_revision();
            let full = !client.entity_sync_initialized
                || client.entity_revision > target_revision
                || client.entity_revision.saturating_add(1) < journal_floor;
            let mut operations = VecDeque::new();
            if full {
                for (entity, _) in &visible {
                    if let Some(records) = entity_records.get(entity) {
                        operations.extend(records.iter().cloned());
                    }
                }
            } else {
                for entity in client
                    .replicated_components
                    .keys()
                    .filter(|entity| !visible_components.contains_key(*entity))
                {
                    operations.push_back(EntityReplicationOperation::Despawn { entity: *entity });
                }
                for (entity, kinds) in &visible_components {
                    let Some(previous) = client.replicated_components.get(entity) else {
                        if let Some(records) = entity_records.get(entity) {
                            operations.extend(records.iter().cloned());
                        }
                        continue;
                    };
                    for kind in previous.difference(kinds) {
                        operations.push_back(EntityReplicationOperation::ComponentRemove {
                            entity: *entity,
                            kind: *kind,
                        });
                    }
                    for kind in kinds.difference(previous) {
                        if let Some(record) = entity_records.get(entity).and_then(|records| records.iter().find(|record| {
                            matches!(record, EntityReplicationOperation::ComponentUpsert { kind: candidate, .. } if candidate == kind)
                        })) {
                            operations.push_back(record.clone());
                        }
                    }
                }
                let mut latest_upserts = std::collections::BTreeMap::new();
                for change in owner.entities.changes_since(client.entity_revision) {
                    if let ComponentMutation::Upsert(entity, kind, blob) = &change.mutation {
                        if client
                            .replicated_components
                            .get(entity)
                            .is_some_and(|previous| previous.contains(kind))
                            && visible_components
                                .get(entity)
                                .is_some_and(|current| current.contains(kind))
                            && registry
                                .descriptor(*kind)
                                .is_some_and(|d| d.replication == ReplicationPolicy::Replicated)
                        {
                            latest_upserts.insert((*entity, *kind), blob);
                        }
                    }
                }
                for ((entity, kind), blob) in latest_upserts {
                    operations.push_back(EntityReplicationOperation::ComponentUpsert {
                        entity,
                        kind,
                        schema: blob.schema,
                        bytes: blob.bytes.clone(),
                    });
                }
            }
            let Some(sync_id) = client
                .next_entity_sync_id
                .checked_add(1)
                .filter(|id| *id > 0)
            else {
                client.disconnect_reason = Some(crate::connection::CloseReason::InvalidSequence);
                continue;
            };
            client.next_entity_sync_id = sync_id;
            client.entity_sync = Some(crate::connection::PendingEntitySync {
                context,
                sync_id,
                base_revision: if full { 0 } else { client.entity_revision },
                target_revision,
                full,
                started: now,
                begin_pending: true,
                operations,
                visible_components,
                complete_sent: false,
            });
        }
        let Some(mut sync) = client.entity_sync.take() else {
            continue;
        };
        let mut budget = ENTITY_SYNC_BUDGET_PER_TICK;
        if sync.begin_pending {
            let message = ServerMessage::EntitySyncBegin {
                context: sync.context,
                sync_id: sync.sync_id,
                base_revision: sync.base_revision,
                target_revision: sync.target_revision,
                full: sync.full,
            };
            if let Some(frame) =
                encode_sync_frame(client, metrics, &message).filter(|frame| frame.len() <= budget)
            {
                let bytes = frame.len();
                if queue_encoded_frame_with_reserve(client, &frame, metrics, MAX_SERVER_FRAME_BYTES)
                {
                    sync.begin_pending = false;
                    budget -= bytes;
                } else {
                    client.entity_sync = Some(sync);
                    continue;
                }
            } else {
                client.entity_sync = Some(sync);
                continue;
            }
        }
        while let Some(operation) = sync.operations.front() {
            let message = ServerMessage::EntitySyncRecord {
                context: sync.context,
                sync_id: sync.sync_id,
                revision: sync.target_revision,
                operation: operation.clone(),
            };
            let Some(frame) = encode_sync_frame(client, metrics, &message) else {
                break;
            };
            let size = frame.len();
            if size > budget {
                break;
            }
            if !queue_encoded_frame_with_reserve(client, &frame, metrics, MAX_SERVER_FRAME_BYTES) {
                break;
            }
            sync.operations.pop_front();
            budget = budget.saturating_sub(size);
        }
        if sync.operations.is_empty() && !sync.complete_sent {
            let message = ServerMessage::EntitySyncComplete {
                context: sync.context,
                sync_id: sync.sync_id,
                target_revision: sync.target_revision,
            };
            if let Some(frame) =
                encode_sync_frame(client, metrics, &message).filter(|frame| frame.len() <= budget)
            {
                if queue_encoded_frame_with_reserve(client, &frame, metrics, MAX_SERVER_FRAME_BYTES)
                {
                    sync.complete_sent = true;
                }
            }
        }
        client.entity_sync = Some(sync);
    }
}

fn encode_sync_frame(
    client: &mut Client,
    metrics: &mut RuntimeMetrics,
    message: &ServerMessage,
) -> Option<Vec<u8>> {
    let started = Instant::now();
    let result = minimium_protocol::encode_server(message);
    let elapsed = started.elapsed();
    metrics.encoding.record(elapsed);
    client.encoding_us = client
        .encoding_us
        .saturating_add(elapsed.as_micros().min(u128::from(u64::MAX)) as u64);
    match result {
        Ok(frame) => Some(frame),
        Err(error) => {
            if let Some(logger) = &client.protocol_logger {
                crate::diagnostics::emit(
                    logger,
                    minimium_diagnostics::Level::Error,
                    "protocol.message_encode_failed",
                    &[("reason", error.to_string())],
                );
            }
            None
        }
    }
}
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
    server_tick: u64,
    resolved_input_seq: &std::collections::HashMap<PlayerId, u64>,
    active_inputs: &std::collections::HashMap<PlayerId, minimium_simulation::MovementIntent>,
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
                        position_mm: player.position_mm.unwrap_or_else(|| {
                            minimium_world::world::Position3Mm(
                                player
                                    .position
                                    .map(|value| (f64::from(value) * 1000.0).round() as i64),
                            )
                        }),
                    });
                },
            );
            let position_mm = snapshot
                .position_mm(recipient)
                .unwrap_or(minimium_world::world::Position3Mm([0; 3]));
            (
                index,
                recipient,
                context,
                character_state,
                position_mm,
                players,
            )
        });
        for (client_index, player_id, context, character_state, position_mm, current_players) in
            snapshots
        {
            if let Some(client) = clients.get_mut(client_index) {
                let full_snapshot = !client.replication_initialized;
                let (players, removed_players) = if full_snapshot {
                    (current_players.clone(), RemotePlayerIds::default())
                } else {
                    diff_remote_players(&client.replicated_players, &current_players)
                };
                let message = ServerMessage::Snapshot {
                    server_time_ms,
                    server_tick,
                    resolved_input_seq: resolved_input_seq.get(&player_id).copied().unwrap_or(0),
                    context,
                    full_snapshot,
                    self_movement: active_inputs.get(&player_id).copied().unwrap_or_default(),
                    self_position: character_state.position,
                    self_position_mm: position_mm,
                    self_velocity: character_state.velocity,
                    self_grounded: character_state.grounded,
                    players: Box::new(players),
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
