use super::{diagnostics::emit_system_effect_diagnostic, ServerRuntime};
use crate::modules::{
    EffectBatchOutcome, EffectOutcomeError, EntityEffectRejection, TransferOutcome,
    TransferRejection, WorldEffect, WorldEffectBatch, WorldEffectOutcome,
};
use std::io;

impl ServerRuntime {
    pub(super) fn report_effect_feedback_diagnostics(
        &self,
        before: super::schedule::ScheduleDiagnostics,
    ) {
        let after = self.schedule.diagnostics();
        let expired = after
            .expired_effect_receipts
            .saturating_sub(before.expired_effect_receipts);
        let backpressured = after
            .effect_backpressure_rejections
            .saturating_sub(before.effect_backpressure_rejections);
        if expired > 0 {
            emit_system_effect_diagnostic(
                &self.world_logger,
                "effect_receipt:expired",
                &format!(
                    "{expired} unread system receipts expired after their delivery invocation"
                ),
            );
        }
        if backpressured > 0 {
            emit_system_effect_diagnostic(
                &self.world_logger,
                "effect_receipt:backpressure",
                &format!("{backpressured} effects refused before acceptance because receipt capacity was full"),
            );
        }
    }

    /// Drains one Startup system's bounded commit receipts for a world.
    /// Startup is not rerun to deliver these receipts.
    pub fn take_startup_effect_receipts(
        &mut self,
        module_id: &'static str,
        system_id: &'static str,
        world: minimium_world::world::WorldInstanceId,
    ) -> Vec<crate::EffectReceipt> {
        self.schedule
            .take_startup_effect_receipts(module_id, system_id, world)
    }

    /// Number of ordinary-system receipts expired unread after their next invocation.
    pub fn expired_effect_receipt_count(&self) -> u64 {
        self.schedule.diagnostics().expired_effect_receipts
    }

    /// Total producer effects refused before acceptance because receipt inboxes were full.
    pub fn backpressured_effect_count(&self) -> u64 {
        self.schedule.diagnostics().effect_backpressure_rejections
    }

    pub(super) fn commit_effect_batch(
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
                let protected_entities = self.impacts.protected_entities();
                let result = self.game.worlds.stage_entity_effect_batch(
                    &entity_effects,
                    &self.component_registry,
                    &protected_entities,
                );
                let (rejection, spawned) = match result {
                    Ok(staged) => {
                        let spawned = staged.spawned_entities().to_vec();
                        match self
                            .operations
                            .player_store
                            .snapshot_budget_mut()
                            .prepare_entity_changes(staged.changes())
                        {
                            Ok(reservation) => {
                                staged.commit(reservation);
                                (None, spawned)
                            }
                            Err(_) => (Some(EntityEffectRejection::StorageBudget), Vec::new()),
                        }
                    }
                    Err(crate::world_registry::RegistryError::SnapshotBudgetExceeded) => {
                        (Some(EntityEffectRejection::StorageBudget), Vec::new())
                    }
                    Err(_) => (Some(EntityEffectRejection::Validation), Vec::new()),
                };
                let mut spawned = spawned.into_iter();
                for index in entity_indices {
                    results[index] = match rejection {
                        Some(reason) => WorldEffectOutcome::EntityRejected(reason),
                        None => match effects[index] {
                            WorldEffect::SpawnEntity { .. } => {
                                let (world, entity) = spawned
                                    .next()
                                    .expect("staged spawn receipt follows input order");
                                WorldEffectOutcome::EntitySpawned { world, entity }
                            }
                            _ => WorldEffectOutcome::EntityApplied,
                        },
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

    pub(super) fn report_effect_batch_outcome(&self, source: &str, outcome: &EffectBatchOutcome) {
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
                    &format!("{source}:{}/{}", reasons[index].0, reasons[index].1),
                    &format!("{count} effects rejected"),
                );
            }
        }
        if outcome.rejected_pushes() > 0 {
            emit_system_effect_diagnostic(
                &self.world_logger,
                &format!("{source}:buffer"),
                &format!(
                    "{} effects rejected by stage limits",
                    outcome.rejected_pushes()
                ),
            );
        }
    }
}
