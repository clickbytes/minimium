use super::*;

impl ServerRuntime {
    pub(in crate::runtime) fn apply_excavation(
        &mut self,
        player: PlayerId,
        context: WorldContext,
        input: ExcavationInput,
    ) {
        if !self
            .game
            .worlds
            .world_input_context_is_current(player, context)
        {
            return;
        }
        match input {
            ExcavationInput::Use {
                action_id,
                direction,
            } => {
                if let Err(reason) = self.prepare_excavation(player, context, action_id, direction)
                {
                    self.excavation_result(
                        player,
                        context,
                        ActionResult::Rejected { action_id, reason },
                    );
                }
            }
            ExcavationInput::Resync { .. } => {
                self.excavation.clients.remove(&player);
            }
            ExcavationInput::GrantTools | ExcavationInput::Equip { .. } => {
                self.change_inventory(player, context, input);
            }
        }
    }

    fn prepare_excavation(
        &mut self,
        player: PlayerId,
        context: WorldContext,
        action_id: u64,
        direction: [f32; 3],
    ) -> Result<(), ActionRejection> {
        if self.excavation.preparing.as_ref().is_some_and(|p| {
            p.player == Some(player) && p.context == Some(context) && p.action_id == Some(action_id)
        }) || self.excavation.pending.as_ref().is_some_and(|p| {
            p.player == Some(player) && p.context == Some(context) && p.action_id == Some(action_id)
        }) {
            return Ok(());
        }
        if let Some((c, gate)) = self.excavation.gates.get(&player) {
            if *c == context {
                if let Some(result) = gate.completed(action_id) {
                    self.excavation_result(player, context, result);
                    return Ok(());
                }
            }
        }
        if self.excavation.failure.is_some() {
            return Err(ActionRejection::Storage);
        }
        if let Some(failure) = self.excavation.water_failures.get(&context.instance()) {
            return Err(if failure.error == minimium_fluids::FluidError::Capacity {
                ActionRejection::Capacity
            } else {
                ActionRejection::Busy
            });
        }
        if self.excavation.busy() {
            return Err(ActionRejection::Busy);
        }
        if self.excavation.water_jobs.contains_key(&context.instance()) {
            return Err(ActionRejection::Busy);
        }
        if self
            .impacts
            .pending
            .keys()
            .any(|(world, _)| *world == context.instance())
        {
            return Err(ActionRejection::Busy);
        }
        let inventory = self.player_inventory(player);
        let inputs = self
            .excavation_inputs(context.instance())
            .ok_or(ActionRejection::Stale)?;
        let input_revision = inputs.revision();
        let owner = self
            .game
            .worlds
            .get(context.instance())
            .ok_or(ActionRejection::Stale)?;
        let manifest = owner.manifest;
        let map = owner.map.clone();
        let store = owner.volumes.clone().ok_or(ActionRejection::Busy)?;
        let terrain =
            scene::planet(manifest, inputs.clone()).ok_or(ActionRejection::InvalidIntent)?;
        let base = terrain
            .volume_base(self.excavation.config.soil_depth_m)
            .ok_or(ActionRejection::InvalidIntent)?;
        let hit = scene::query(
            owner,
            player,
            &store,
            &base,
            direction,
            self.excavation.config.tools.reach_m,
        );
        let gate = self.excavation.gates.entry(player).or_insert_with(|| {
            (
                context,
                minimium_simulation::tools::ToolUseGate::new(self.excavation.config.tools).unwrap(),
            )
        });
        if gate.0 != context {
            *gate = (
                context,
                minimium_simulation::tools::ToolUseGate::new(self.excavation.config.tools).unwrap(),
            );
        }
        let action_result = gate.1.prepare(
            ToolUseIntent {
                world: context.instance(),
                action_id,
                direction: direction.map(f64::from),
            },
            inventory,
            &scene::FirstHit(hit),
            self.scheduler.server_started.elapsed().as_millis() as u64,
        );
        let action = match action_result {
            Ok(action) => action,
            Err(reason @ ActionRejection::Obstructed) => {
                crate::diagnostics::emit(
                    &self.world_logger,
                    minimium_diagnostics::Level::Warn,
                    "excavation.static_first_hit",
                    &[("world", context.instance().to_string())],
                );
                return Err(reason);
            }
            Err(reason) => return Err(reason),
        };
        let id = self
            .excavation
            .allocate_id()
            .ok_or(ActionRejection::Capacity)?;
        let request = TerrainEditRequest {
            mutation_id: id,
            center_m: action.hit.position_m,
            radius_m: action.radius_m,
            tool: action.tool,
        };
        let soil_depth_m = self.excavation.config.soil_depth_m;
        let water = self.excavation.waters.get(&context.instance()).cloned();
        let water_catalog_digest = self
            .excavation
            .water_catalogs
            .get(&context.instance())
            .copied();
        let other_cells: usize = self
            .excavation
            .waters
            .iter()
            .filter(|(world, _)| **world != context.instance())
            .map(|(_, water)| water.reserved_cell_limit())
            .sum();
        let water_budget = self
            .excavation
            .config
            .server_fluid_cells
            .saturating_sub(other_cells)
            .min(self.excavation.config.fluids.max_active_cells);
        let job = preparation::ToolPreparation {
            request,
            manifest,
            map,
            inputs,
            store,
            water,
            water_budget,
            water_catalog_digest,
            soil_depth_m,
        };
        let (sender, receiver) = std::sync::mpsc::sync_channel(1);
        self.sector_workers
            .prepare(Box::new(move || {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| job.run()))
                    .unwrap_or(Err(ActionRejection::Capacity.into()));
                let _ = sender.send(result);
            }))
            .map_err(|_| ActionRejection::Busy)?;
        self.excavation.preparing = Some(Preparation {
            started: Instant::now(),
            player: Some(player),
            context: Some(context),
            action_id: Some(action_id),
            world: context.instance(),
            impact_bodies: Vec::new(),
            input_revision,
            receiver,
        });
        Ok(())
    }
}
