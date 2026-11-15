use super::*;

impl ServerRuntime {
    pub(super) fn change_inventory(
        &mut self,
        player: PlayerId,
        context: WorldContext,
        input: ExcavationInput,
    ) {
        let Some(profile) = self.excavation_profile(player) else {
            return;
        };
        let committed = self.player_inventory(player);
        // Equipment presses can arrive while a terrain or water checkpoint is
        // writing. Keep one desired state per connected player instead of losing
        // the press. Include an already submitted grant when applying Equip.
        let submitted = self
            .excavation
            .pending
            .as_ref()
            .and_then(|pending| pending.prepared.mutation.inventory)
            .filter(|(owner, _)| *owner == profile)
            .map(|(_, inventory)| inventory);
        let old = self
            .excavation
            .inventory_requests
            .get(&player)
            .filter(|(epoch, _)| *epoch == context)
            .map(|(_, inventory)| *inventory)
            .or(submitted)
            .unwrap_or(committed);
        let mut inventory = old;
        let changed = match input {
            ExcavationInput::GrantTools => inventory.ensure_tools().map(|_| ()),
            ExcavationInput::Equip { slot } => inventory.equip(slot),
            _ => return,
        };
        if changed.is_err() || self.excavation.failure.is_some() {
            self.send_excavation(
                player,
                context,
                ExcavationMessage::Inventory {
                    inventory: committed,
                },
            );
            return;
        }
        if old != inventory {
            self.excavation
                .inventory_requests
                .insert(player, (context, inventory));
        } else if submitted.is_none() && !self.excavation.inventory_requests.contains_key(&player) {
            self.send_excavation(player, context, ExcavationMessage::Inventory { inventory });
        }
        self.flush_inventory_request();
    }

    pub(super) fn flush_inventory_request(&mut self) {
        self.excavation
            .inventory_requests
            .retain(|player, (context, _)| {
                self.game
                    .worlds
                    .world_input_context_is_current(*player, *context)
            });
        if self.excavation.busy()
            || self.excavation.failure.is_some()
            || self.excavation.shutting_down
        {
            return;
        }
        let now = Instant::now();
        if self
            .excavation
            .water_checkpoint_due
            .iter()
            .any(|(world, deadline)| *deadline <= now && self.excavation.waters[world].dirty())
        {
            // Repeated equipment presses must not postpone coherent fluid
            // checkpoints indefinitely. update_water owns the next journal slot.
            return;
        }
        let Some((&player, &(context, mut inventory))) =
            self.excavation.inventory_requests.iter().next()
        else {
            return;
        };
        self.excavation.inventory_requests.remove(&player);
        let Some(profile) = self.excavation_profile(player) else {
            return;
        };
        let committed = self.player_inventory(player);
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
        let mutation = WorldMutation {
            id,
            world: context.instance(),
            world_seed: owner.manifest.generation().seed().get(),
            expected_chunk_revisions: Vec::new(),
            resulting_chunks: Vec::new(),
            inventory: Some((profile, inventory)),
            fluids: None,
            impact_bodies: Vec::new(),
            craters: Vec::new(),
            water_catalog_digest: None,
        };
        if let Err(error) = self.excavation.journal.submit(mutation.clone()) {
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
        self.excavation.pending = Some(Pending {
            started: Instant::now(),
            player: Some(player),
            context: Some(context),
            action_id: None,
            prepared: Prepared {
                mutation,
                terrain: None,
                sectors: Default::default(),
                collision_elapsed: Duration::ZERO,
                water: None,
                craters: None,
            },
        });
    }
}
