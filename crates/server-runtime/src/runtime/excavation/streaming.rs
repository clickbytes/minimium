use super::*;
use minimium_protocol::ServerErrorCode;
use minimium_world::terrain::TerrainChunkKey;
use replication::{ChunkReplicationCursor, FluidReplicationCursor};

pub(super) struct Cursor {
    context: WorldContext,
    anchor: [i32; 3],
    revision: u64,
    inventory: Option<Inventory>,
    sync_id: u64,
    stream: Option<ChunkReplicationCursor>,
    waiting: Option<ExcavationMessage>,
    visible: std::collections::BTreeMap<[i32; 3], u64>,
    fluid_stream: Option<FluidReplicationCursor>,
    fluid_revision: Option<u64>,
    fluid_region: Option<([i32; 3], [i32; 3])>,
    batch_end: Option<ExcavationMessage>,
    crater_ids: HashSet<u64>,
}

impl ServerRuntime {
    pub(super) fn stream_excavation(&mut self) {
        let mut rejected_players = Vec::new();
        for client in &mut self.connections.clients {
            let Some(identity) = client.identity() else {
                continue;
            };
            let player = identity.player;
            let Some(context) = self.game.worlds.context_for_player(player) else {
                continue;
            };
            let Some(owner) = self.game.worlds.get(context.instance()) else {
                continue;
            };
            let Some(actor) = owner.world.player(player) else {
                continue;
            };
            let position = actor
                .position_mm
                .map_or(actor.position.map(f64::from), |p| {
                    p.0.map(|v| v as f64 / 1000.0)
                });
            let Some(key) = TerrainChunkKey::at(context.instance(), position) else {
                continue;
            };
            let revision = owner.volumes.as_ref().map_or(0, |s| s.last_mutation());
            let fluid_region = region_for_chunk(key.coordinates);
            let cursor = self
                .excavation
                .clients
                .entry(player)
                .or_insert_with(|| Cursor::new(context, key.coordinates));
            if cursor.context != context {
                *cursor = Cursor::new(context, key.coordinates);
            }
            if let Some((result_context, result)) =
                self.excavation.action_results.get(&player).copied()
            {
                if result_context != context {
                    self.excavation.action_results.remove(&player);
                } else {
                    let queued = crate::connection::queue_message(
                        client,
                        &mut self.metrics,
                        ServerMessage::Excavation {
                            context,
                            message: ExcavationMessage::ActionResult { result },
                        },
                    );
                    if queued {
                        self.excavation.action_results.remove(&player);
                    } else {
                        continue;
                    }
                }
            }
            let inventory = self
                .excavation
                .inventories
                .get(&identity.principal.profile())
                .copied()
                .unwrap_or_default();
            if cursor.inventory != Some(inventory) {
                if !crate::connection::queue_message(
                    client,
                    &mut self.metrics,
                    ServerMessage::Excavation {
                        context,
                        message: ExcavationMessage::Inventory { inventory },
                    },
                ) {
                    continue;
                }
                cursor.inventory = Some(inventory);
            }
            let water = self.excavation.waters.get(&context.instance());
            if cursor.stream.is_none() && cursor.fluid_stream.is_none() {
                // Keep replicas already seen by this client. Dropping them on an
                // interest change would restore the original surface at far LOD.
                let mut changed: Vec<_> = owner
                    .volumes
                    .iter()
                    .flat_map(|s| s.chunks())
                    .filter(|chunk| {
                        let coordinates = chunk.key().coordinates;
                        let nearby =
                            (0..3).all(|i| coordinates[i].abs_diff(key.coordinates[i]) <= 1);
                        (nearby || cursor.visible.contains_key(&coordinates))
                            && cursor.visible.get(&coordinates) != Some(&chunk.revision().0)
                    })
                    .cloned()
                    .collect();
                changed.sort_by_key(|chunk| {
                    chunk
                        .key()
                        .coordinates
                        .iter()
                        .zip(key.coordinates)
                        .map(|(a, b)| a.abs_diff(b))
                        .max()
                        .unwrap_or(0)
                });
                changed.truncate(64);
                if cursor.revision != revision
                    || cursor.anchor != key.coordinates
                    || !changed.is_empty()
                {
                    let full = cursor.revision == u64::MAX;
                    let Some(sync_id) = cursor.sync_id.checked_add(1) else {
                        continue;
                    };
                    let craters = owner
                        .terrain
                        .edits()
                        .iter()
                        .filter(|stamp| full || !cursor.crater_ids.contains(&stamp.id))
                        .copied()
                        .collect();
                    let stream = ChunkReplicationCursor::new(
                        sync_id,
                        if full { 0 } else { cursor.revision },
                        revision,
                        full,
                        changed.clone(),
                        owner.terrain.radius_m(),
                        craters,
                    );
                    if let Some(stream) = stream {
                        let mut fluid_stream = None;
                        let mut fluid_revision = None;
                        if let Some(water) = water.filter(|water| {
                            Some(water.snapshot().revision()) != cursor.fluid_revision
                                || cursor.fluid_region != Some(fluid_region)
                                || full
                        }) {
                            let fluid_revision_value = water.snapshot().revision();
                            let Some(prepared_fluid) = FluidReplicationCursor::new(
                                sync_id,
                                water.snapshot_arc(),
                                fluid_region.0,
                                fluid_region.1,
                            ) else {
                                reject_oversized_fluid_replica(client, &mut self.metrics);
                                rejected_players.push(player);
                                continue;
                            };
                            fluid_stream = Some(prepared_fluid);
                            fluid_revision = Some(fluid_revision_value);
                        }
                        cursor.stream = Some(stream);
                        cursor.sync_id = sync_id;
                        cursor.fluid_stream = fluid_stream;
                        cursor.crater_ids =
                            owner.terrain.edits().iter().map(|stamp| stamp.id).collect();
                        for chunk in changed {
                            cursor
                                .visible
                                .insert(chunk.key().coordinates, chunk.revision().0);
                        }
                        cursor.revision = revision;
                        cursor.anchor = key.coordinates;
                        if let Some(fluid_revision) = fluid_revision {
                            cursor.fluid_revision = Some(fluid_revision);
                            cursor.fluid_region = Some(fluid_region);
                        }
                    }
                } else if let Some(water) = water.filter(|water| {
                    Some(water.snapshot().revision()) != cursor.fluid_revision
                        || cursor.fluid_region != Some(fluid_region)
                }) {
                    let Some(sync_id) = cursor.sync_id.checked_add(1) else {
                        continue;
                    };
                    let Some(fluid_stream) = FluidReplicationCursor::new(
                        sync_id,
                        water.snapshot_arc(),
                        fluid_region.0,
                        fluid_region.1,
                    ) else {
                        reject_oversized_fluid_replica(client, &mut self.metrics);
                        rejected_players.push(player);
                        continue;
                    };
                    cursor.sync_id = sync_id;
                    cursor.fluid_stream = Some(fluid_stream);
                    cursor.fluid_revision = Some(water.snapshot().revision());
                    cursor.fluid_region = Some(fluid_region);
                }
            }
            for _ in 0..4 {
                if client.pending_output_bytes() > 32 * 1024 {
                    break;
                }
                let Some(message) = cursor.next_message() else {
                    break;
                };
                let queued_before = client.pending_output_bytes();
                if !crate::connection::queue_message(
                    client,
                    &mut self.metrics,
                    ServerMessage::Excavation {
                        context,
                        message: message.clone(),
                    },
                ) {
                    cursor.waiting = Some(message);
                    break;
                }
                self.excavation.metrics.bytes = self.excavation.metrics.bytes.saturating_add(
                    client.pending_output_bytes().saturating_sub(queued_before) as u64,
                );
            }
            if cursor.waiting.is_none()
                && cursor.batch_end.is_none()
                && cursor.stream.as_ref().is_some_and(|s| s.is_done())
            {
                cursor.stream = None;
            }
            if cursor.waiting.is_none() && cursor.fluid_stream.as_ref().is_some_and(|s| s.is_done())
            {
                cursor.fluid_stream = None;
            }
        }
        for player in rejected_players {
            self.excavation.clients.remove(&player);
        }
    }
}

fn reject_oversized_fluid_replica(
    client: &mut crate::connection::Client,
    metrics: &mut crate::metrics::RuntimeMetrics,
) {
    if let Some(logger) = &client.protocol_logger {
        crate::diagnostics::emit(
            logger,
            minimium_diagnostics::Level::Error,
            "server.fluid_replication_capacity",
            &[(
                "reason",
                "regional snapshot exceeds protocol byte limit".into(),
            )],
        );
    }
    let _ = crate::connection::queue_close_response(
        client,
        metrics,
        ServerMessage::Error(ServerErrorCode::Busy),
        Instant::now(),
    );
}

impl Cursor {
    fn new(context: WorldContext, anchor: [i32; 3]) -> Self {
        Self {
            context,
            anchor,
            revision: u64::MAX,
            inventory: None,
            sync_id: 0,
            stream: None,
            waiting: None,
            visible: Default::default(),
            fluid_stream: None,
            fluid_revision: None,
            fluid_region: None,
            batch_end: None,
            crater_ids: HashSet::new(),
        }
    }

    fn next_message(&mut self) -> Option<ExcavationMessage> {
        if let Some(message) = self.waiting.take() {
            return Some(message);
        }
        if self.batch_end.is_some() {
            return self
                .fluid_stream
                .as_mut()
                .and_then(|stream| stream.next())
                .or_else(|| self.batch_end.take());
        }
        if let Some(message) = self.stream.as_mut().and_then(|stream| stream.next()) {
            if matches!(message, ExcavationMessage::BatchEnd { .. }) && self.fluid_stream.is_some()
            {
                self.batch_end = Some(message);
                return self.next_message();
            }
            return Some(message);
        }
        self.fluid_stream.as_mut().and_then(|stream| stream.next())
    }
}

fn region_for_chunk(chunk: [i32; 3]) -> ([i32; 3], [i32; 3]) {
    let minimum = chunk.map(|coordinate| coordinate * 32 - 16);
    let maximum = chunk.map(|coordinate| coordinate * 32 + 48);
    (minimum, maximum)
}
