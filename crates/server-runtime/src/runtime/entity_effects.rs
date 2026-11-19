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
                &format!(
                    "{backpressured} effects refused before acceptance because receipt capacity was full"
                ),
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
                let result = self.game.worlds.stage_entity_effect_batch(
                    &entity_effects,
                    &self.component_registry,
                    &self.component_owner_authority,
                    None,
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
                    Err(error) => (Some(entity_effect_rejection(error)), Vec::new()),
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
                    if matches!(transfer, TransferOutcome::Rejected(_)) {
                        self.game.pending_portal_transitions.remove(player);
                        if let Some((source_context, sequence)) =
                            self.game.pending_portal_receipts.remove(player)
                        {
                            self.send_player_action_receipt(
                                *player,
                                sequence,
                                source_context,
                                minimium_protocol::PlayerActionKind::Portal,
                                minimium_protocol::PlayerActionDecision::Rejected(5),
                            );
                        }
                    }
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

    /// Applies an owner-authenticated entity batch through the same atomic
    /// staging and storage-reservation path used by ordinary effects.
    pub fn apply_owned_component_effects(
        &mut self,
        port: &super::ComponentOwnerPort,
        effects: &[WorldEffect],
    ) -> Result<(), super::ComponentOwnerError> {
        use super::ComponentOwnerError as OwnerError;

        if self.shutdown_request.is_requested() {
            return Err(OwnerError::Revoked);
        }
        if !self.component_registry.contains_owner(port.owner()) {
            return Err(OwnerError::ForeignPort);
        }
        self.component_owner_authority
            .validate(port, port.owner(), port.world())?;
        if effects.is_empty() {
            return Ok(());
        }
        if effects.len() > crate::modules::MAX_WORLD_EFFECTS_PER_STAGE {
            return Err(OwnerError::InvalidMutation);
        }
        let mut payload_bytes = 0usize;
        for effect in effects {
            let bytes = match effect {
                WorldEffect::SpawnEntity { components, .. } => {
                    if components.len() > minimium_world::entity::MAX_COMPONENTS_PER_ENTITY {
                        return Err(OwnerError::InvalidMutation);
                    }
                    components
                        .iter()
                        .try_fold(0usize, |total, component| {
                            total.checked_add(component.bytes.len())
                        })
                        .ok_or(OwnerError::InvalidMutation)?
                }
                WorldEffect::SetComponent { component, .. } => component.bytes.len(),
                _ => 0,
            };
            payload_bytes = payload_bytes
                .checked_add(bytes)
                .filter(|total| *total <= crate::modules::MAX_WORLD_EFFECT_BYTES_PER_STAGE)
                .ok_or(OwnerError::InvalidMutation)?;
        }
        if effects
            .iter()
            .any(|effect| matches!(effect, WorldEffect::RequestTransfer { .. }))
        {
            return Err(OwnerError::InvalidMutation);
        }
        let staged = self
            .game
            .worlds
            .stage_entity_effect_batch(
                effects,
                &self.component_registry,
                &self.component_owner_authority,
                Some(port),
            )
            .map_err(|error| match error {
                crate::world_registry::RegistryError::SnapshotBudgetExceeded => {
                    OwnerError::StorageBudget
                }
                _ => OwnerError::Validation,
            })?;
        let reservation = self
            .operations
            .player_store
            .snapshot_budget_mut()
            .prepare_entity_changes(staged.changes())
            .map_err(|_| OwnerError::StorageBudget)?;
        staged.commit(reservation);
        Ok(())
    }

    pub(super) fn report_effect_batch_outcome(&self, source: &str, outcome: &EffectBatchOutcome) {
        let mut counts = [0usize; 13];
        for result in outcome.results() {
            let index = match result {
                WorldEffectOutcome::EntityRejected(EntityEffectRejection::Validation) => Some(0),
                WorldEffectOutcome::EntityRejected(EntityEffectRejection::StorageBudget) => Some(1),
                WorldEffectOutcome::EntityRejected(EntityEffectRejection::UnknownInstance) => {
                    Some(8)
                }
                WorldEffectOutcome::EntityRejected(EntityEffectRejection::UnknownEntity) => Some(9),
                WorldEffectOutcome::EntityRejected(EntityEffectRejection::ProtectedComponent) => {
                    Some(10)
                }
                WorldEffectOutcome::EntityRejected(EntityEffectRejection::ReservationConflict) => {
                    Some(11)
                }
                WorldEffectOutcome::EntityRejected(
                    EntityEffectRejection::IdentifierSpaceExhausted,
                ) => Some(12),
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
            ("entity", "unknown_instance"),
            ("entity", "unknown_entity"),
            ("entity", "protected_component"),
            ("entity", "reservation_conflict"),
            ("entity", "identifier_space_exhausted"),
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

fn entity_effect_rejection(error: crate::world_registry::RegistryError) -> EntityEffectRejection {
    use crate::world_registry::RegistryError;
    use minimium_world::entity::ComponentError;
    match error {
        RegistryError::UnknownInstance => EntityEffectRejection::UnknownInstance,
        RegistryError::EntitySectorConflict
        | RegistryError::Component(ComponentError::UnknownEntity) => {
            EntityEffectRejection::UnknownEntity
        }
        RegistryError::ProtectedComponent => EntityEffectRejection::ProtectedComponent,
        RegistryError::EntityReservationConflict => EntityEffectRejection::ReservationConflict,
        RegistryError::MembershipRevisionExhausted
        | RegistryError::Component(
            ComponentError::RevisionExhausted | ComponentError::ExhaustedIds,
        ) => EntityEffectRejection::IdentifierSpaceExhausted,
        RegistryError::SnapshotBudgetExceeded => EntityEffectRejection::StorageBudget,
        _ => EntityEffectRejection::Validation,
    }
}
