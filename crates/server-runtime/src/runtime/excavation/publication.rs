use super::*;

impl ServerRuntime {
    pub(in crate::runtime) fn restore_excavation(&mut self) -> io::Result<()> {
        let restored = std::mem::take(&mut self.excavation.restored);
        for (world, saved) in &restored.worlds {
            if self
                .game
                .worlds
                .get(*world)
                .is_none_or(|owner| owner.manifest.generation().seed().get() != saved.world_seed)
            {
                return Err(io::Error::other(
                    "excavation checkpoint world identity mismatch",
                ));
            }
        }
        for (world, owner) in self.game.worlds.iter_mut() {
            if owner.manifest.planet().is_none() {
                continue;
            }
            let saved = restored.worlds.get(&world);
            let store = TerrainStore::from_chunks(
                world,
                self.excavation.config.terrain,
                saved.map_or(0, |s| s.last_mutation),
                saved.into_iter().flat_map(|s| s.chunks.values().cloned()),
            )
            .ok_or_else(|| io::Error::other("invalid excavation terrain checkpoint"))?;
            owner.volumes = Some(Arc::new(store));
        }
        self.restore_water(&restored)?;
        Ok(())
    }

    pub(in crate::runtime) fn update_excavation(&mut self) {
        if let Some(preparation) = self.excavation.preparing.take() {
            match preparation.receiver.try_recv() {
                Ok(result) => self.finish_excavation_preparation(preparation, result),
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    self.excavation.preparing = Some(preparation);
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.reject_preparation(preparation, ActionRejection::Capacity);
                }
            }
        }
        while let Some(result) = self.excavation.journal.poll_durable() {
            let Some(pending) = self.excavation.pending.take() else {
                self.excavation.failure =
                    Some("unexpected durable excavation acknowledgement".into());
                break;
            };
            match result {
                Ok(mutation) if mutation.id == pending.prepared.mutation.id => {
                    self.excavation
                        .metrics
                        .persistence
                        .record(pending.started.elapsed());
                    self.install_excavation(pending);
                }
                result => {
                    let reason = match result {
                        Err(error) => {
                            let detail = error.to_string();
                            let reason = self.excavation_storage_rejection(error);
                            if !pending.prepared.mutation.impact_bodies.is_empty() {
                                self.clear_pending_impacts(&pending.prepared.mutation);
                                self.impact_failure(io::Error::other(format!(
                                    "impact mutation was rejected by durable storage: {detail}"
                                )));
                            }
                            reason
                        }
                        Ok(_) => {
                            self.excavation.failure =
                                Some("excavation acknowledgement identity mismatch".into());
                            if !pending.prepared.mutation.impact_bodies.is_empty() {
                                self.clear_pending_impacts(&pending.prepared.mutation);
                                self.impact_failure(io::Error::other(
                                    "impact mutation acknowledgement identity mismatch",
                                ));
                            }
                            ActionRejection::Storage
                        }
                    };
                    if let (Some(player), Some(context), Some(action_id)) =
                        (pending.player, pending.context, pending.action_id)
                    {
                        self.excavation_result(
                            player,
                            context,
                            ActionResult::Rejected { action_id, reason },
                        );
                    }
                }
            }
        }
        self.flush_inventory_request();
        self.update_water();
        self.stream_excavation();
        let active: HashSet<_> = self
            .connections
            .clients
            .iter()
            .filter_map(|c| c.player_id())
            .collect();
        self.excavation
            .clients
            .retain(|player, _| active.contains(player));
        self.excavation
            .gates
            .retain(|player, _| active.contains(player));
        self.excavation
            .action_results
            .retain(|player, _| active.contains(player));
    }

    fn finish_excavation_preparation(
        &mut self,
        preparation: Preparation,
        result: Result<Prepared, PreparationError>,
    ) {
        self.excavation
            .metrics
            .prepare
            .record(preparation.started.elapsed());
        let result = match result {
            Err(PreparationError::Geometry(failure)) => {
                crate::diagnostics::emit(
                    &self.world_logger,
                    minimium_diagnostics::Level::Warn,
                    "world.excavation_geometry_rejected",
                    &[
                        ("reason", format!("{:?}", failure.reason)),
                        ("mutation_id", failure.mutation_id.to_string()),
                        (
                            "action_id",
                            preparation
                                .action_id
                                .map_or_else(|| "none".to_owned(), |id| id.to_string()),
                        ),
                        (
                            "center_m",
                            format!(
                                "{:.1},{:.1},{:.1}",
                                failure.center_m[0], failure.center_m[1], failure.center_m[2]
                            ),
                        ),
                        ("chunks", format!("{:?}", failure.chunk_coordinates)),
                        ("world", preparation.world.to_string()),
                    ],
                );
                Err(ActionRejection::UnsafeGeometry)
            }
            Err(PreparationError::Rejected(reason)) => Err(reason),
            Ok(prepared) => Ok(prepared),
        };
        let result = result.and_then(|prepared| {
            if preparation
                .player
                .zip(preparation.context)
                .is_some_and(|(player, context)| {
                    !self
                        .game
                        .worlds
                        .world_input_context_is_current(player, context)
                })
                || self
                    .excavation_inputs(preparation.world)
                    .is_none_or(|inputs| inputs.revision() != preparation.input_revision)
            {
                return Err(ActionRejection::Stale);
            }
            self.excavation
                .journal
                .validate(&prepared.mutation)
                .map_err(|error| self.excavation_storage_rejection(error))?;
            self.excavation
                .journal
                .submit(prepared.mutation.clone())
                .map_err(|error| self.excavation_storage_rejection(error))?;
            Ok(prepared)
        });
        match result {
            Ok(prepared) => {
                self.excavation.pending = Some(Pending {
                    started: Instant::now(),
                    player: preparation.player,
                    context: preparation.context,
                    action_id: preparation.action_id,
                    prepared,
                });
            }
            Err(reason) => self.reject_preparation(preparation, reason),
        }
    }

    fn reject_preparation(&mut self, preparation: Preparation, reason: ActionRejection) {
        if let (Some(player), Some(context), Some(action_id)) = (
            preparation.player,
            preparation.context,
            preparation.action_id,
        ) {
            self.excavation_result(
                player,
                context,
                ActionResult::Rejected { action_id, reason },
            );
        }
        for body in preparation.impact_bodies {
            self.impacts.pending.remove(&(body.world, body.id));
            if !matches!(reason, ActionRejection::Busy | ActionRejection::Stale) {
                self.impact_failure(io::Error::other(format!(
                    "impact geometry refused: {reason:?}"
                )));
            }
        }
    }

    fn clear_pending_impacts(&mut self, mutation: &WorldMutation) {
        for body in &mutation.impact_bodies {
            self.impacts.pending.remove(&(body.world, body.id));
        }
    }

    fn install_excavation(&mut self, pending: Pending) {
        let started = Instant::now();
        let mutation = &pending.prepared.mutation;
        let before = self.excavation_inputs(mutation.world).unwrap();
        for body in &mutation.impact_bodies {
            if let Err(error) = self.validate_impact(*body) {
                self.excavation.failure = Some(error.to_string());
                self.shutdown_request.request();
                return;
            }
        }
        if let Some(terrain) = pending.prepared.terrain {
            let owner = self.game.worlds.get(mutation.world).unwrap();
            let manifest = owner.manifest;
            let Some(collision_revision) = owner.collision_source_revision.checked_add(1) else {
                self.excavation.failure = Some("collision revision exhausted".into());
                self.shutdown_request.request();
                return;
            };
            let sector_revision = minimium_world::world::SectorRevision::new(mutation.id).unwrap();
            if self
                .game
                .sectors
                .publish_prepared(
                    mutation.world,
                    manifest.generation().revision(),
                    manifest.content_revision(),
                    sector_revision,
                    &pending.prepared.sectors,
                )
                .is_err()
            {
                self.excavation.failure = Some("sector ticket space exhausted".into());
                self.shutdown_request.request();
                return;
            }
            for body in &mutation.impact_bodies {
                if let Err(error) = self.install_impact_prepared(*body) {
                    self.excavation.failure = Some(error.to_string());
                    self.shutdown_request.request();
                    return;
                }
            }
            let mut after = before.clone();
            after.volumes = Some(terrain.clone());
            after.remnants = self
                .game
                .worlds
                .get(mutation.world)
                .map(|owner| owner.remnants.clone());
            if let Some(craters) = &pending.prepared.craters {
                after.edits = Some(craters.clone());
            }
            self.game.sectors.invalidate_where(mutation.world, |key| {
                !pending.prepared.sectors.contains_key(&key)
                    && before.revision() != after.revision()
            });
            let tickets: HashSet<_> = self
                .game
                .sectors
                .installed()
                .filter(|(ticket, _, _)| ticket.instance() == mutation.world)
                .map(|(ticket, _, _)| ticket)
                .collect();
            let parts = tickets
                .iter()
                .filter_map(|ticket| {
                    self.game
                        .sectors
                        .payload_arc(*ticket)
                        .map(|p| p.collision.clone())
                })
                .collect();
            let index = Arc::new(minimium_simulation::CollisionIndex::from_partitions(parts));
            let replacements = tickets
                .iter()
                .filter_map(|ticket| {
                    self.game
                        .sectors
                        .payload_arc(*ticket)
                        .map(|payload| (ticket.key(), Arc::clone(&payload.collision)))
                })
                .collect();
            let owner = self.game.worlds.get_mut(mutation.world).unwrap();
            owner.volumes = Some(terrain);
            if let Some(craters) = pending.prepared.craters {
                owner.terrain = craters;
            }
            owner
                .world
                .install_streamed_spatial_geometry(index, replacements, collision_revision);
            owner.collision_tickets = tickets;
            owner.collision_source_revision = collision_revision;
            owner.collision_installed_revision = collision_revision;
            self.excavation
                .metrics
                .collision
                .record(pending.prepared.collision_elapsed);
        }
        if let Some((profile, inventory)) = mutation.inventory {
            self.excavation.inventories.insert(profile, inventory);
        }
        for body in &mutation.impact_bodies {
            self.impacts.pending.remove(&(body.world, body.id));
            self.impacts.bodies.insert((body.world, body.id), *body);
        }
        if let Some(water) = pending.prepared.water {
            self.excavation.waters.insert(mutation.world, water);
        }
        if let Some(snapshot) = &mutation.fluids {
            if let Some(water) = self.excavation.waters.get_mut(&mutation.world) {
                water.mark_persisted(snapshot.revision());
                self.game
                    .worlds
                    .get_mut(mutation.world)
                    .unwrap()
                    .world
                    .set_water_read(Some(water.water_read()));
            }
        }
        if let (Some(player), Some(context), Some(action_id)) =
            (pending.player, pending.context, pending.action_id)
        {
            self.excavation_result(
                player,
                context,
                ActionResult::Committed {
                    action_id,
                    mutation_id: mutation.id,
                },
            );
        }
        self.excavation
            .metrics
            .publication
            .record(started.elapsed());
    }

    pub(in crate::runtime) fn shutdown_excavation(&mut self, deadline: Instant) -> io::Result<()> {
        // Unsubmitted preparation has no durable side effects. Its bounded result
        // receiver is dropped; the shared worker pool joins during lifecycle shutdown.
        self.excavation.preparing = None;
        self.excavation.shutting_down = true;
        self.excavation.water_jobs.clear();
        self.drain_excavation(deadline)?;
        let dirty: Vec<_> = self
            .excavation
            .waters
            .iter()
            .filter_map(|(world, water)| water.dirty().then_some(*world))
            .collect();
        for world in dirty {
            self.submit_water_checkpoint(world)?;
            self.drain_excavation(deadline)?;
        }
        self.excavation.journal.shutdown_until(deadline)?;
        self.update_excavation();
        if let Some(error) = &self.excavation.failure {
            return Err(io::Error::other(error.clone()));
        }
        Ok(())
    }

    fn drain_excavation(&mut self, deadline: Instant) -> io::Result<()> {
        while self.excavation.pending.is_some() {
            self.update_excavation();
            if Instant::now() >= deadline {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "excavation persistence deadline",
                ));
            }
            if self.excavation.pending.is_some() {
                std::thread::sleep(Duration::from_millis(5));
            }
        }
        if let Some(error) = &self.excavation.failure {
            return Err(io::Error::other(error.clone()));
        }
        Ok(())
    }
}
