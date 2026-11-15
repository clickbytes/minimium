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
                self.excavation.state.clients.remove(&player);
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
        if self
            .excavation
            .coordinator
            .contains_action(player, context, action_id)
        {
            return Ok(());
        }
        if let Some((c, gate)) = self.excavation.state.gates.get(&player) {
            if *c == context {
                if let Some(result) = gate.completed(action_id) {
                    self.excavation_result(player, context, result);
                    return Ok(());
                }
            }
        }
        if self.excavation.coordinator.failure().is_some() {
            return Err(ActionRejection::Storage);
        }
        if let Some(failure) = self
            .excavation
            .state
            .water_failures
            .get(&context.instance())
        {
            return Err(if failure.error == minimium_fluids::FluidError::Capacity {
                ActionRejection::Capacity
            } else {
                ActionRejection::Busy
            });
        }
        if self.excavation.busy() {
            return Err(ActionRejection::Busy);
        }
        if self
            .excavation
            .state
            .water_jobs
            .contains_key(&context.instance())
        {
            return Err(ActionRejection::Busy);
        }
        if self.impacts.has_pending_in_world(context.instance()) {
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
        let query_reach = inventory
            .tool()
            .and_then(|tool| self.excavation.state.config.tools.definition(tool))
            .map_or(
                self.excavation.state.config.tools.max_reach_m(),
                |definition| definition.reach_m(),
            );
        let terrain =
            scene::planet(manifest, inputs.clone()).ok_or(ActionRejection::InvalidIntent)?;
        let base = terrain
            .volume_base()
            .ok_or(ActionRejection::InvalidIntent)?;
        let hit = scene::query(owner, player, &store, &base, direction, query_reach);
        let gate = self
            .excavation
            .state
            .gates
            .entry(player)
            .or_insert_with(|| {
                (
                    context,
                    minimium_simulation::tools::ToolUseGate::new(
                        self.excavation.state.config.tools,
                    )
                    .unwrap(),
                )
            });
        if gate.0 != context {
            *gate = (
                context,
                minimium_simulation::tools::ToolUseGate::new(self.excavation.state.config.tools)
                    .unwrap(),
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
            self.now
                .saturating_duration_since(self.scheduler.server_started)
                .as_millis() as u64,
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
        let deformation = minimium_terrain::TerrainDeformation::new(
            action.definition.radius_m(),
            action.definition.allowed_materials(),
        )
        .map_err(|_| ActionRejection::InvalidIntent)?;
        let request = TerrainEditRequest {
            mutation_id: id,
            center_m: action.hit.position_m,
            deformation,
        };
        let water = self
            .excavation
            .state
            .waters
            .get(&context.instance())
            .cloned();
        let water_catalog_digest = self
            .excavation
            .state
            .water_catalogs
            .get(&context.instance())
            .copied();
        let other_cells: usize = self
            .excavation
            .state
            .waters
            .iter()
            .filter(|(world, _)| **world != context.instance())
            .map(|(_, water)| water.reserved_cell_limit())
            .sum();
        let water_budget = self
            .excavation
            .state
            .config
            .server_fluid_cells
            .saturating_sub(other_cells)
            .min(self.excavation.state.config.fluids.max_active_cells);
        let job = preparation::ToolPreparation {
            request,
            manifest,
            map,
            inputs,
            store,
            water,
            water_budget,
            water_catalog_digest,
        };
        self.excavation
            .coordinator
            .prepare(
                PreparationRequest {
                    origin: Origin {
                        player: Some(player),
                        context: Some(context),
                        action_id: Some(action_id),
                    },
                    world: context.instance(),
                    impact_bodies: Vec::new(),
                    input_revision,
                },
                Box::new(move || job.run()),
                &mut ports::WorkSubmission::new(&self.sector_workers, &mut self.impacts),
            )
            .map_err(|_| ActionRejection::Busy)?;
        Ok(())
    }
}
