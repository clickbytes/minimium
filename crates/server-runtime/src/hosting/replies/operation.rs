use super::{CommandPayloadView, CommandReplyView, ReplyState};
use minimium_commands::operation::CommandValue as Value;
use minimium_commands::results::{
    OperationContext, OperationFailure, OperationReceipt, OperationResult, Stage,
};
use minimium_gameplay::{
    EffectBatchOutcome, EffectProducerId, TransferOutcome, WorldEffectOutcome,
};
use std::collections::BTreeMap;

impl CommandReplyView<'_> {
    pub fn stage(&self) -> Stage {
        match self.state {
            ReplyState::Rejected if self.error_code == Some("server.stopping") => Stage::Cancelled,
            ReplyState::Rejected => Stage::Rejected,
            ReplyState::Partial => Stage::Partial,
            ReplyState::Executed => match &self.payload {
                Some(CommandPayloadView::SaveQueued) => Stage::OwnerAccepted,
                Some(CommandPayloadView::ModuleText { .. }) => Stage::LegacyReported,
                Some(CommandPayloadView::Bolid { .. } | CommandPayloadView::ImpactSpawn { .. }) => {
                    Stage::OwnerAccepted
                }
                Some(CommandPayloadView::Stopping) => Stage::OwnerCommitted,
                Some(
                    CommandPayloadView::ClockCommitted(_)
                    | CommandPayloadView::EntityDespawn { .. },
                ) => Stage::OwnerCommitted,
                _ => Stage::Completed,
            },
        }
    }

    /// Borrow the original exhaustive payload, keeping pagination and partial results available.
    pub fn operation_result(self, identity: OperationContext) -> OperationResult<Self> {
        let mut receipt = OperationReceipt::at(self.stage());
        receipt.should_stop = self.should_stop;
        if let Some(CommandPayloadView::Property(report)) = &self.payload {
            receipt.revision = report.value().revision;
            receipt.freshness = Some(report.value().freshness);
        }
        if let Some(CommandPayloadView::Time(clock) | CommandPayloadView::ClockCommitted(clock)) =
            &self.payload
        {
            receipt.revision = Some(clock.revision());
            receipt.freshness = Some(minimium_commands::results::property::Freshness::Current);
        }
        if let Some(outcome) = self.effects {
            receipt.effects = effects(outcome);
        }
        let data = if self.state == ReplyState::Rejected {
            Err(OperationFailure {
                code: self.error_code.unwrap_or("server.command.failed").into(),
                subject: self.error_subject.map(str::to_owned),
                reason: self.detail.unwrap_or("command rejected").into(),
                usage: None,
                span: None,
                origin: "ServerRuntime".into(),
            })
        } else {
            Ok(self)
        };
        OperationResult::new(identity, receipt, data)
    }
}

fn effects(batch: &EffectBatchOutcome) -> Vec<Value> {
    let mut records = Vec::new();
    for (index, result) in batch.results().iter().enumerate() {
        let mut fields = BTreeMap::new();
        let state = match result {
            WorldEffectOutcome::EntityApplied => "entity_applied",
            WorldEffectOutcome::EntitySpawned { world, entity } => {
                fields.insert("world".into(), Value::U128(world.get()));
                fields.insert("entity".into(), Value::U64(entity.get()));
                "entity_spawned"
            }
            WorldEffectOutcome::EntityRejected(reason) => {
                fields.insert(
                    "reason".into(),
                    Value::Enum(crate::command_output::entity_rejection_label(*reason).into()),
                );
                "entity_rejected"
            }
            WorldEffectOutcome::Transfer(TransferOutcome::Started) => "transfer_started",
            WorldEffectOutcome::Transfer(TransferOutcome::Rejected(reason)) => {
                fields.insert(
                    "reason".into(),
                    Value::Enum(crate::command_output::transfer_rejection_label(*reason).into()),
                );
                "transfer_rejected"
            }
            WorldEffectOutcome::StageRejected => "stage_rejected",
        };
        fields.insert("state".into(), Value::Enum(state.into()));
        if let Some(receipt) = batch.receipts().get(index) {
            let token = receipt.token;
            fields.insert("sequence".into(), Value::U64(token.sequence()));
            let (kind, module, name) = match token.producer() {
                EffectProducerId::System { module, system } => ("system", module, system),
                EffectProducerId::Command { module, command } => ("command", module, command),
                EffectProducerId::Startup {
                    module,
                    system,
                    world,
                } => {
                    fields.insert("producer_world".into(), Value::U128(world.get()));
                    ("startup", module, system)
                }
            };
            fields.insert("producer_kind".into(), Value::Enum(kind.into()));
            fields.insert("module".into(), Value::Text(module.into()));
            fields.insert("producer".into(), Value::Text(name.into()));
        }
        records.push(Value::Record(fields));
    }
    if batch.rejected_pushes() != 0 {
        records.push(Value::Record(
            [(
                "rejected_pushes".into(),
                Value::U64(batch.rejected_pushes() as u64),
            )]
            .into(),
        ));
    }
    records
}
