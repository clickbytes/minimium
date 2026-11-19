use super::*;

impl ServerRuntime {
    pub(in crate::runtime) fn restore_excavation(&mut self) -> io::Result<()> {
        let restored = std::mem::take(&mut self.excavation.state.restored);
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
                self.excavation.state.config.terrain,
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
        let target_tick = self.step_boundary().tick();
        self.excavation
            .coordinator
            .advance(&mut ports::Publication::new(
                target_tick,
                &mut self.excavation.state,
                &mut self.equipment,
                &mut self.game.worlds,
                &mut self.game.sectors,
                &mut self.impacts,
                &self.component_registry,
                &self.component_owner_authority,
                &self.prepared_geography,
                &self.world_logger,
                &self.shutdown_request,
            ));
        self.flush_inventory_request();
        self.update_water();
    }

    pub(in crate::runtime) fn publish_excavation_stream(&mut self) {
        self.stream_excavation();
        let active: HashSet<_> = self
            .connections
            .clients
            .iter()
            .filter_map(|c| c.player_id())
            .collect();
        self.excavation
            .state
            .clients
            .retain(|player, _| active.contains(player));
        self.equipment
            .retain_actors(|player| active.contains(&player));
        self.excavation
            .state
            .action_results
            .retain(|player, _| active.contains(player));
    }

    pub(in crate::runtime) fn shutdown_excavation(&mut self, deadline: Instant) -> io::Result<()> {
        // Unsubmitted preparation has no durable side effects. Its bounded result
        // receiver is dropped; the shared worker pool joins during lifecycle shutdown.
        self.excavation
            .coordinator
            .begin_shutdown(&mut ports::WorkSubmission::new(
                &self.sector_workers,
                &mut self.impacts,
            ));
        self.excavation.state.water_jobs.clear();
        self.drain_excavation(deadline)?;
        let dirty: Vec<_> = self
            .excavation
            .state
            .waters
            .iter()
            .filter_map(|(world, water)| water.dirty().then_some(*world))
            .collect();
        for world in dirty {
            self.submit_water_checkpoint(world)?;
            self.drain_excavation(deadline)?;
        }
        self.excavation.coordinator.shutdown_journal(deadline)?;
        self.update_excavation();
        if let Some(error) = self.excavation.coordinator.failure() {
            return Err(io::Error::other(error.to_owned()));
        }
        Ok(())
    }

    fn drain_excavation(&mut self, deadline: Instant) -> io::Result<()> {
        while self.excavation.coordinator.awaiting_durability() {
            self.update_excavation();
            if Instant::now() >= deadline {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "excavation persistence deadline",
                ));
            }
            if self.excavation.coordinator.awaiting_durability() {
                std::thread::sleep(Duration::from_millis(5));
            }
        }
        if let Some(error) = self.excavation.coordinator.failure() {
            return Err(io::Error::other(error.to_owned()));
        }
        Ok(())
    }
}
