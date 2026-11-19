use super::*;

impl ServerRuntime {
    pub(in crate::runtime) fn change_inventory(
        &mut self,
        player: PlayerId,
        context: WorldContext,
        input: ExcavationInput,
    ) -> bool {
        let Some(profile) = self.player_profile(player) else {
            return false;
        };
        let committed = self.equipment.confirmed(profile);
        if self.excavation.coordinator.failure().is_some() {
            self.send_excavation(
                player,
                context,
                ExcavationMessage::Inventory {
                    inventory: committed,
                },
            );
            return false;
        }
        let outcome = self.equipment.request_change(
            player,
            context,
            profile,
            input,
            self.excavation.state.config.tool_grants,
        );
        let Ok(response) = outcome else {
            self.send_excavation(
                player,
                context,
                ExcavationMessage::Inventory {
                    inventory: committed,
                },
            );
            return false;
        };
        if let Some(inventory) = response {
            self.send_excavation(player, context, ExcavationMessage::Inventory { inventory });
        }
        self.flush_inventory_request();
        true
    }

    pub(super) fn flush_inventory_request(&mut self) {
        self.equipment.retain_requests(|player, context| {
            self.game
                .worlds
                .world_input_context_is_current(player, context)
        });
        if self.excavation.busy()
            || self.excavation.coordinator.failure().is_some()
            || self.excavation.coordinator.shutting_down()
        {
            return;
        }
        let now = self.now;
        if self
            .excavation
            .state
            .water_checkpoint_due
            .iter()
            .any(|(world, deadline)| {
                *deadline <= now && self.excavation.state.waters[world].dirty()
            })
        {
            // Repeated equipment presses must not postpone coherent fluid
            // checkpoints indefinitely. update_water owns the next journal slot.
            return;
        }
        let Some((player, context, mut inventory)) = self.equipment.next_request() else {
            return;
        };
        self.equipment.remove_request(player);
        let Some(profile) = self.player_profile(player) else {
            return;
        };
        let committed = self.equipment.confirmed(profile);
        if inventory.slots == committed.slots && inventory.equipped == committed.equipped {
            return;
        }
        let Some(revision) = committed.revision.checked_add(1) else {
            return;
        };
        inventory.revision = revision;
        let Some(id) = self.excavation.allocate_id() else {
            return;
        };
        let Some(owner) = self.game.worlds.get(context.instance()) else {
            return;
        };
        let mutation = match WorldChangePlan::new(
            id,
            context.instance(),
            owner.manifest.generation().seed().get(),
            Vec::new(),
            Vec::new(),
            Some((profile, inventory)),
            None,
            Vec::new(),
            Vec::new(),
            None,
        ) {
            Ok(plan) => plan,
            Err(_) => return,
        };
        let prepared = Prepared {
            mutation,
            terrain: None,
            sectors: Default::default(),
            collision_elapsed: Duration::ZERO,
            water: None,
            craters: None,
        };
        let origin = Origin {
            player: Some(player),
            context: Some(context),
            action_id: None,
        };
        if self.equipment.reserve_plan(&prepared.mutation).is_err() {
            self.send_excavation(
                player,
                context,
                ExcavationMessage::Inventory {
                    inventory: committed,
                },
            );
            return;
        }
        let mutation_id = prepared.mutation.id();
        if let Err(error) = self.excavation.coordinator.submit(prepared, origin) {
            self.equipment.cancel_plan(mutation_id);
            self.excavation_storage_rejection(error);
            self.send_excavation(
                player,
                context,
                ExcavationMessage::Inventory {
                    inventory: committed,
                },
            );
            return;
        }
    }
}
