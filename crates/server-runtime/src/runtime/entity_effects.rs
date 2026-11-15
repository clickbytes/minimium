use super::{diagnostics::emit_system_effect_diagnostic, ServerRuntime};
use std::io;

impl ServerRuntime {
    pub(super) fn commit_system_batch(
        &mut self,
        batch: crate::modules::WorldEffectBatch,
    ) -> io::Result<()> {
        let mut transfers = Vec::new();
        for effect in &batch.effects {
            if let crate::modules::WorldEffect::RequestTransfer {
                player,
                source_world,
                target_world,
                destination,
            } = effect
            {
                let context = self
                    .game
                    .worlds
                    .context_for_player(*player)
                    .filter(|context| context.instance() == *source_world)
                    .ok_or_else(|| {
                        io::Error::new(
                            io::ErrorKind::InvalidData,
                            "gameplay transfer request has stale world context",
                        )
                    })?;
                transfers.push((
                    *player,
                    context,
                    *target_world,
                    *destination,
                    self.operations.player_store.return_location_for(*player),
                ));
            }
        }
        if !self.game.worlds.validate_transfer_batch(&transfers) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "gameplay transfer batch is invalid",
            ));
        }
        let entity_effects: Vec<_> = batch
            .effects
            .iter()
            .filter(|effect| !matches!(effect, crate::modules::WorldEffect::RequestTransfer { .. }))
            .cloned()
            .collect();
        let staged = self
            .game
            .worlds
            .stage_entity_effect_batch(&entity_effects, &self.component_registry);
        let commit_result = match staged {
            Ok(staged) => match self
                .operations
                .player_store
                .snapshot_budget_mut()
                .prepare_entity_changes(staged.changes())
            {
                Ok(reservation) => {
                    staged.commit(reservation);
                    Ok(())
                }
                Err(_) => Err("SnapshotBudgetExceeded".to_owned()),
            },
            Err(error) => Err(format!("{error:?}")),
        };
        if let Err(error) = commit_result {
            emit_system_effect_diagnostic(&self.world_logger, "stage_batch", &error);
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "gameplay effect batch rejected",
            ));
        }
        for (player, context, target_world, destination, _) in transfers {
            self.begin_entity_transfer(player, context, target_world, destination);
        }
        Ok(())
    }
}
