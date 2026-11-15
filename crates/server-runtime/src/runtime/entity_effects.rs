use super::{diagnostics::emit_system_effect_diagnostic, ServerRuntime};
use crate::modules::{
    EffectBatchOutcome, EffectOutcomeError, EntityEffectRejection, TransferOutcome,
    TransferRejection, WorldEffect, WorldEffectBatch, WorldEffectOutcome,
};
use std::io;

impl ServerRuntime {
    pub(super) fn commit_system_batch(
        &mut self,
        batch: WorldEffectBatch,
    ) -> io::Result<EffectBatchOutcome> {
        let effects = batch.effects();
        let mut results = vec![WorldEffectOutcome::StageRejected; effects.len()];

        if batch.rejected_pushes() == 0 {
            let entity_indices: Vec<_> = effects
                .iter()
                .enumerate()
                .filter_map(|(index, effect)| {
                    (!matches!(effect, WorldEffect::RequestTransfer { .. })).then_some(index)
                })
                .collect();
            let entity_effects: Vec<_> = entity_indices
                .iter()
                .map(|index| effects[*index].clone())
                .collect();

            if !entity_effects.is_empty() {
                let result = self
                    .game
                    .worlds
                    .stage_entity_effect_batch(&entity_effects, &self.component_registry);
                let rejection = match result {
                    Ok(staged) => match self
                        .operations
                        .player_store
                        .snapshot_budget_mut()
                        .prepare_entity_changes(staged.changes())
                    {
                        Ok(reservation) => {
                            staged.commit(reservation);
                            None
                        }
                        Err(_) => Some(EntityEffectRejection::StorageBudget),
                    },
                    Err(crate::world_registry::RegistryError::SnapshotBudgetExceeded) => {
                        Some(EntityEffectRejection::StorageBudget)
                    }
                    Err(_) => Some(EntityEffectRejection::Validation),
                };
                for index in entity_indices {
                    results[index] = match rejection {
                        Some(reason) => WorldEffectOutcome::EntityRejected(reason),
                        None => WorldEffectOutcome::EntityApplied,
                    };
                }
            }

            for (index, effect) in effects.iter().enumerate() {
                if let WorldEffect::RequestTransfer {
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
                        .filter(|context| context.instance() == *source_world);
                    let transfer = match context {
                        Some(context) => self.begin_entity_transfer(
                            *player,
                            context,
                            *target_world,
                            *destination,
                        ),
                        None => TransferOutcome::Rejected(TransferRejection::Validation),
                    };
                    results[index] = WorldEffectOutcome::Transfer(transfer);
                }
            }
        }

        batch.into_outcome(results).map_err(|error| match error {
            EffectOutcomeError::ResultCountMismatch => io::Error::other(
                "gameplay effect outcome count did not match the bounded input batch",
            ),
        })
    }

    pub(super) fn report_system_batch_outcome(
        &self,
        stage: crate::modules::Stage,
        outcome: &EffectBatchOutcome,
    ) {
        let mut counts = [0usize; 8];
        for result in outcome.results() {
            let index = match result {
                WorldEffectOutcome::EntityRejected(EntityEffectRejection::Validation) => Some(0),
                WorldEffectOutcome::EntityRejected(EntityEffectRejection::StorageBudget) => Some(1),
                WorldEffectOutcome::Transfer(TransferOutcome::Rejected(
                    TransferRejection::Validation,
                )) => Some(2),
                WorldEffectOutcome::Transfer(TransferOutcome::Rejected(
                    TransferRejection::AlreadyPending,
                )) => Some(3),
                WorldEffectOutcome::Transfer(TransferOutcome::Rejected(
                    TransferRejection::IdentifierSpaceExhausted,
                )) => Some(4),
                WorldEffectOutcome::Transfer(TransferOutcome::Rejected(
                    TransferRejection::ClientUnavailable,
                )) => Some(5),
                WorldEffectOutcome::Transfer(TransferOutcome::Rejected(
                    TransferRejection::EnvironmentUnavailable,
                )) => Some(6),
                WorldEffectOutcome::Transfer(TransferOutcome::Rejected(
                    TransferRejection::Backpressure,
                )) => Some(7),
                _ => None,
            };
            if let Some(index) = index {
                counts[index] = counts[index].saturating_add(1);
            }
        }

        let reasons = [
            ("entity", "validation"),
            ("entity", "storage_budget"),
            ("transfer", "validation"),
            ("transfer", "already_pending"),
            ("transfer", "identifier_space_exhausted"),
            ("transfer", "client_unavailable"),
            ("transfer", "environment_unavailable"),
            ("transfer", "backpressure"),
        ];
        for (index, count) in counts.into_iter().enumerate() {
            if count > 0 {
                emit_system_effect_diagnostic(
                    &self.world_logger,
                    &format!("{stage:?}:{}/{}", reasons[index].0, reasons[index].1),
                    &format!("{count} effects rejected"),
                );
            }
        }
        if outcome.rejected_pushes() > 0 {
            emit_system_effect_diagnostic(
                &self.world_logger,
                &format!("{stage:?}:buffer"),
                &format!(
                    "{} effects rejected by stage limits",
                    outcome.rejected_pushes()
                ),
            );
        }
    }
}
