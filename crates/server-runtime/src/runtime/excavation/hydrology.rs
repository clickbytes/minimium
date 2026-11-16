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
    pub(super) fn restore_water(&mut self, restored: &WorldChangeState) -> io::Result<()> {
        for (world, owner) in self.game.worlds.iter_mut() {
            if owner.manifest.planet().is_none() {
                continue;
            }
            owner
                .resolved_relief_policy
                .ok_or_else(|| io::Error::other("water catalog relief policy unavailable"))?;
            let source = minimium_procgen::PreparedTerrainSource::new(
                world,
                owner.manifest,
                &owner.map,
                self.prepared_geography.clone(),
                owner.terrain_material_profile,
                owner.resolved_relief_policy,
            )
            .map_err(io::Error::other)?;
            let inputs = minimium_procgen::TerrainInputs::new(
                source,
                minimium_procgen::TerrainOverlaySnapshot::empty_for(world),
            )
            .map_err(io::Error::other)?;
            let terrain = scene::planet(owner.manifest, inputs)
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
            self.excavation
                .state
                .water_catalogs
                .insert(world, catalog.digest);
            let mut water = if let Some(snapshot) = saved.and_then(|s| s.fluids.clone()) {
                if snapshot.cells().len() > self.excavation.state.config.fluids.max_active_cells {
                    return Err(io::Error::other(
                        "saved water exceeds configured world cell budget",
                    ));
                }
                water::WaterWorld::with_restored_snapshot(snapshot, Arc::new(catalog))
            } else {
                let level = (terrain.radius_mm() as f64 + terrain.sea_level_mm() as f64) / 1000.0;
                water::WaterWorld::from_catalog(
                    world,
                    self.excavation.state.config.fluids,
                    &catalog,
                    level,
                )
                .map_err(|error| io::Error::other(format!("water catalog: {error:?}")))?
            };
            water
                .reconfigure(self.excavation.state.config.fluids)
                .map_err(|error| io::Error::other(format!("water configuration: {error:?}")))?;
            water
                .reserve_cell_limit(water.snapshot().cells().len())
                .map_err(|error| io::Error::other(format!("water reservation: {error:?}")))?;
            owner.world.set_water_read(Some(water.water_read()));
            self.excavation.state.waters.insert(world, water);
            self.excavation.state.water_checkpoint_due.insert(
                world,
                self.now
                    + self
                        .excavation
                        .state
                        .async_work_policy
                        .water_checkpoint_cadence(),
            );
        }
        let cells: usize = self
            .excavation
            .state
            .waters
            .values()
            .map(|w| w.snapshot().cells().len())
            .sum();
        if cells > self.excavation.state.config.server_fluid_cells {
            return Err(io::Error::other(
                "saved water exceeds configured server cell budget",
            ));
        }
        let mut remaining = self.excavation.state.config.server_fluid_cells - cells;
        let mut worlds: Vec<_> = self.excavation.state.waters.keys().copied().collect();
        worlds.sort_unstable();
        for world in worlds {
            let water = self.excavation.state.waters.get_mut(&world).unwrap();
            let occupied = water.snapshot().cells().len();
            if occupied == 0 {
                continue;
            }
            let maximum = self
                .excavation
                .state
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
        if self.excavation.coordinator.shutting_down()
            || self.excavation.coordinator.failure().is_some()
        {
            return;
        }
        self.publish_water_jobs();
        if self.excavation.coordinator.failure().is_some() {
            return;
        }
        let now = self.now;
        if now >= self.excavation.state.next_water_step {
            self.excavation.state.next_water_step =
                now + self.excavation.state.async_work_policy.hydrology_cadence();
            let worlds: Vec<_> = self.excavation.state.waters.keys().copied().collect();
            for world in worlds {
                if !self.excavation_reserves(world)
                    && !self.excavation.state.water_jobs.contains_key(&world)
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
                .state
                .water_checkpoint_due
                .iter()
                .filter(|(world, deadline)| {
                    **deadline <= now && self.excavation.state.waters[world].dirty()
                })
                .min_by_key(|(_, deadline)| **deadline)
                .map(|(world, _)| *world);
            if let Some(world) = due {
                if let Err(error) = self.submit_water_checkpoint(world) {
                    self.water_failure(format!("water checkpoint refused: {error}"));
                } else {
                    self.excavation.state.water_checkpoint_due.insert(
                        world,
                        now + self
                            .excavation
                            .state
                            .async_work_policy
                            .water_checkpoint_cadence(),
                    );
                }
            }
        }
    }

    fn publish_water_jobs(&mut self) {
        let jobs = std::mem::take(&mut self.excavation.state.water_jobs);
        for (world, job) in jobs {
            match job.receiver.try_recv() {
                Ok(Ok((water, elapsed))) => {
                    let current = self
                        .excavation
                        .state
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
                        self.excavation.state.waters.insert(world, water);
                        self.excavation.state.water_failures.remove(&world);
                        self.excavation.state.metrics.solver.record(elapsed);
                    }
                }
                Ok(Err(error)) => {
                    self.water_step_failure(world, error);
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    self.excavation.state.water_jobs.insert(world, job);
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
            .state
            .water_failures
            .get(&world)
            .is_some_and(|failure| self.now < failure.retry_at)
        {
            return;
        }
        let water = &self.excavation.state.waters[&world];
        if water.snapshot().cells().is_empty() || water.sleeping() {
            return;
        }
        let mut water = water.clone();
        let revision = water.snapshot().revision();
        let Some(inputs) = self.excavation_inputs(world) else {
            self.water_step_failure(world, minimium_fluids::FluidError::Unavailable);
            return;
        };
        let input_revision = inputs.revision();
        let owner = self.game.worlds.get(world).unwrap();
        let manifest = owner.manifest;
        let store = owner.volumes.clone().unwrap();
        let async_work_policy = self.excavation.state.async_work_policy;
        let (sender, receiver) = std::sync::mpsc::sync_channel(1);
        if self
            .sector_workers
            .prepare(Box::new(move || {
                let started = Instant::now();
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    let terrain = scene::planet(manifest, inputs)
                        .ok_or(minimium_fluids::FluidError::Unavailable)?;
                    let base = terrain
                        .volume_base()
                        .ok_or(minimium_fluids::FluidError::Invalid)?;
                    water.step_terrain(
                        &store.view(&base),
                        [0.0; 3],
                        9.81,
                        async_work_policy.hydrology_fixed_step().as_secs_f64(),
                    )?;
                    Ok((water, started.elapsed()))
                }))
                .unwrap_or(Err(minimium_fluids::FluidError::Unavailable));
                let _ = sender.send(result);
            }))
            .is_ok()
        {
            self.excavation.state.water_jobs.insert(
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
        self.excavation.coordinator.fail(reason);
    }

    fn water_step_failure(&mut self, world: WorldInstanceId, error: minimium_fluids::FluidError) {
        if self
            .excavation
            .state
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
        self.excavation.state.water_failures.insert(
            world,
            WaterFailure {
                error,
                retry_at: self.now
                    + self
                        .excavation
                        .state
                        .async_work_policy
                        .hydrology_retry_delay(),
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
        let mutation = WorldChangePlan::new(
            id,
            world,
            owner.manifest.generation().seed().get(),
            Vec::new(),
            Vec::new(),
            None,
            Some(self.excavation.state.waters[&world].snapshot().clone()),
            Vec::new(),
            Vec::new(),
            self.excavation.state.water_catalogs.get(&world).copied(),
        )
        .map_err(|_| io::Error::other("invalid world change plan"))?;
        self.excavation
            .coordinator
            .submit(
                Prepared {
                    mutation,
                    terrain: None,
                    sectors: Default::default(),
                    collision_elapsed: Duration::ZERO,
                    water: None,
                    craters: None,
                },
                Origin::default(),
            )
            .map_err(io::Error::other)
    }
}
