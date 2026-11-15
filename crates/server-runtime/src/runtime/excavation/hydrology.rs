use super::*;

pub(super) struct WaterJob {
    input_revision: minimium_procgen::SectorContentVersion,
    water_revision: u64,
    receiver: Receiver<Result<(water::WaterWorld, Duration), minimium_fluids::FluidError>>,
}

pub(super) struct WaterFailure {
    pub error: minimium_fluids::FluidError,
    retry_at: Instant,
}

impl ServerRuntime {
    pub(super) fn restore_water(&mut self, restored: &ExcavationRestore) -> io::Result<()> {
        for (world, owner) in self.game.worlds.iter_mut() {
            if owner.manifest.planet().is_none() {
                continue;
            }
            let terrain = scene::planet(
                owner.manifest,
                minimium_procgen::TerrainInputs::from(self.prepared_geography.clone()),
            )
            .ok_or_else(|| io::Error::other("water catalog planet unavailable"))?;
            let catalog = minimium_procgen::WaterCatalog::build(
                &terrain,
                minimium_procgen::DEFAULT_WATER_CATALOG_RESOLUTION,
            )
            .map_err(io::Error::other)?;
            let saved = restored.worlds.get(&world);
            if saved
                .and_then(|s| s.water_catalog_digest)
                .is_some_and(|digest| digest != catalog.digest)
            {
                return Err(io::Error::other(
                    "water catalog integrity mismatch; no migration supported",
                ));
            }
            self.excavation.water_catalogs.insert(world, catalog.digest);
            let mut water = if let Some(snapshot) = saved.and_then(|s| s.fluids.clone()) {
                if snapshot.cells().len() > self.excavation.config.fluids.max_active_cells {
                    return Err(io::Error::other(
                        "saved water exceeds configured world cell budget",
                    ));
                }
                water::WaterWorld::with_restored_snapshot(snapshot, Arc::new(catalog))
            } else {
                let level = (terrain.radius_mm() as f64 + terrain.sea_level_mm() as f64) / 1000.0;
                water::WaterWorld::from_catalog(
                    world,
                    self.excavation.config.fluids,
                    &catalog,
                    level,
                )
                .map_err(|error| io::Error::other(format!("water catalog: {error:?}")))?
            };
            water
                .reconfigure(self.excavation.config.fluids)
                .map_err(|error| io::Error::other(format!("water configuration: {error:?}")))?;
            water
                .reserve_cell_limit(water.snapshot().cells().len())
                .map_err(|error| io::Error::other(format!("water reservation: {error:?}")))?;
            owner.world.set_water_read(Some(water.water_read()));
            self.excavation.waters.insert(world, water);
            self.excavation
                .water_checkpoint_due
                .insert(world, Instant::now() + Duration::from_secs(1));
        }
        let cells: usize = self
            .excavation
            .waters
            .values()
            .map(|w| w.snapshot().cells().len())
            .sum();
        if cells > self.excavation.config.server_fluid_cells {
            return Err(io::Error::other(
                "saved water exceeds configured server cell budget",
            ));
        }
        let mut remaining = self.excavation.config.server_fluid_cells - cells;
        let mut worlds: Vec<_> = self.excavation.waters.keys().copied().collect();
        worlds.sort_unstable();
        for world in worlds {
            let water = self.excavation.waters.get_mut(&world).unwrap();
            let occupied = water.snapshot().cells().len();
            if occupied == 0 {
                continue;
            }
            let maximum = self
                .excavation
                .config
                .fluids
                .max_active_cells
                .min(water.snapshot().config().max_active_cells);
            let extra = maximum.saturating_sub(occupied).min(remaining);
            water
                .reserve_cell_limit(occupied + extra)
                .map_err(|error| io::Error::other(format!("water reservation: {error:?}")))?;
            remaining -= extra;
        }
        Ok(())
    }

    pub(super) fn update_water(&mut self) {
        if self.excavation.shutting_down || self.excavation.failure.is_some() {
            return;
        }
        self.publish_water_jobs();
        if self.excavation.failure.is_some() {
            return;
        }
        let now = Instant::now();
        if now >= self.excavation.next_water_step {
            self.excavation.next_water_step = now + Duration::from_millis(100);
            let worlds: Vec<_> = self.excavation.waters.keys().copied().collect();
            for world in worlds {
                if !self.excavation_reserves(world)
                    && !self.excavation.water_jobs.contains_key(&world)
                {
                    self.queue_water_step(world);
                }
            }
        }
        if !self.excavation.busy() {
            // Select the oldest due world; a busy lake cannot starve the other
            // worlds by resetting a single server-wide checkpoint deadline.
            let due = self
                .excavation
                .water_checkpoint_due
                .iter()
                .filter(|(world, deadline)| {
                    **deadline <= now && self.excavation.waters[world].dirty()
                })
                .min_by_key(|(_, deadline)| **deadline)
                .map(|(world, _)| *world);
            if let Some(world) = due {
                if let Err(error) = self.submit_water_checkpoint(world) {
                    self.water_failure(format!("water checkpoint refused: {error}"));
                } else {
                    self.excavation
                        .water_checkpoint_due
                        .insert(world, now + Duration::from_secs(1));
                }
            }
        }
    }

    fn publish_water_jobs(&mut self) {
        let jobs = std::mem::take(&mut self.excavation.water_jobs);
        for (world, job) in jobs {
            match job.receiver.try_recv() {
                Ok(Ok((water, elapsed))) => {
                    let current = self
                        .excavation
                        .waters
                        .get(&world)
                        .map(|w| w.snapshot().revision());
                    if current == Some(job.water_revision)
                        && self
                            .excavation_inputs(world)
                            .is_some_and(|inputs| inputs.revision() == job.input_revision)
                    {
                        self.game
                            .worlds
                            .get_mut(world)
                            .unwrap()
                            .world
                            .set_water_read(Some(water.water_read()));
                        self.excavation.waters.insert(world, water);
                        self.excavation.water_failures.remove(&world);
                        self.excavation.metrics.solver.record(elapsed);
                    }
                }
                Ok(Err(error)) => {
                    self.water_step_failure(world, error);
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    self.excavation.water_jobs.insert(world, job);
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.water_step_failure(world, minimium_fluids::FluidError::Unavailable);
                }
            }
        }
    }

    fn queue_water_step(&mut self, world: WorldInstanceId) {
        if self
            .excavation
            .water_failures
            .get(&world)
            .is_some_and(|failure| Instant::now() < failure.retry_at)
        {
            return;
        }
        let water = &self.excavation.waters[&world];
        if water.snapshot().cells().is_empty() || water.sleeping() {
            return;
        }
        let mut water = water.clone();
        let revision = water.snapshot().revision();
        let inputs = self.excavation_inputs(world).unwrap();
        let input_revision = inputs.revision();
        let owner = self.game.worlds.get(world).unwrap();
        let manifest = owner.manifest;
        let store = owner.volumes.clone().unwrap();
        let soil_depth_m = self.excavation.config.soil_depth_m;
        let (sender, receiver) = std::sync::mpsc::sync_channel(1);
        if self
            .sector_workers
            .prepare(Box::new(move || {
                let started = Instant::now();
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    let terrain = scene::planet(manifest, inputs)
                        .ok_or(minimium_fluids::FluidError::Unavailable)?;
                    let base = terrain
                        .volume_base(soil_depth_m)
                        .ok_or(minimium_fluids::FluidError::Invalid)?;
                    water.step_terrain(&store.view(&base), [0.0; 3], 9.81, 0.1)?;
                    Ok((water, started.elapsed()))
                }))
                .unwrap_or(Err(minimium_fluids::FluidError::Unavailable));
                let _ = sender.send(result);
            }))
            .is_ok()
        {
            self.excavation.water_jobs.insert(
                world,
                WaterJob {
                    input_revision,
                    water_revision: revision,
                    receiver,
                },
            );
        }
    }

    fn water_failure(&mut self, reason: String) {
        emit_system_effect_diagnostic(&self.world_logger, "water", &reason);
        self.excavation.failure = Some(reason);
    }

    fn water_step_failure(&mut self, world: WorldInstanceId, error: minimium_fluids::FluidError) {
        if self
            .excavation
            .water_failures
            .get(&world)
            .is_none_or(|failure| failure.error != error)
        {
            emit_system_effect_diagnostic(
                &self.world_logger,
                "water",
                &format!(
                    "world {world:?}: step refused ({error:?}); retaining complete water state"
                ),
            );
        }
        self.excavation.water_failures.insert(
            world,
            WaterFailure {
                error,
                retry_at: Instant::now() + Duration::from_secs(5),
            },
        );
    }

    pub(super) fn submit_water_checkpoint(&mut self, world: WorldInstanceId) -> io::Result<()> {
        let id = self
            .excavation
            .allocate_id()
            .ok_or_else(|| io::Error::other("mutation IDs exhausted"))?;
        let owner = self
            .game
            .worlds
            .get(world)
            .ok_or_else(|| io::Error::other("unknown water world"))?;
        let mutation = WorldMutation {
            id,
            world,
            world_seed: owner.manifest.generation().seed().get(),
            expected_chunk_revisions: Vec::new(),
            resulting_chunks: Vec::new(),
            inventory: None,
            fluids: Some(self.excavation.waters[&world].snapshot().clone()),
            impact_bodies: Vec::new(),
            craters: Vec::new(),
            water_catalog_digest: self.excavation.water_catalogs.get(&world).copied(),
        };
        self.excavation.journal.submit(mutation.clone())?;
        self.excavation.pending = Some(Pending {
            player: None,
            context: None,
            action_id: None,
            started: Instant::now(),
            prepared: Prepared {
                mutation,
                terrain: None,
                sectors: Default::default(),
                collision_elapsed: Duration::ZERO,
                water: None,
                craters: None,
            },
        });
        Ok(())
    }
}
