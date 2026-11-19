//! Volatile, bounded correlation from reservation to durable owner publication.

use super::*;
use minimium_protocol::WorldContext;
use std::collections::VecDeque;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ImpactReceiptStage {
    OwnerAccepted,
    OwnerCommitted,
    Failed,
}

impl ImpactReceiptStage {
    pub fn label(self) -> &'static str {
        match self {
            Self::OwnerAccepted => "owner_accepted",
            Self::OwnerCommitted => "owner_committed",
            Self::Failed => "failed",
        }
    }
}

#[derive(Clone, Debug)]
pub struct ImpactReceipt {
    pub(crate) id: u64,
    pub(crate) world: WorldInstanceId,
    pub(crate) observer: PlayerId,
    pub(crate) context: WorldContext,
    pub(crate) requested_tick: u64,
    pub(crate) stage: ImpactReceiptStage,
    pub(crate) journal_revision: Option<u64>,
    pub(crate) committed_tick: Option<u64>,
    pub(crate) reason: Option<String>,
}

impl ImpactReceipt {
    pub fn id(&self) -> u64 {
        self.id
    }
    pub fn world(&self) -> WorldInstanceId {
        self.world
    }
    pub fn observer(&self) -> PlayerId {
        self.observer
    }
    pub fn context(&self) -> WorldContext {
        self.context
    }
    pub fn requested_tick(&self) -> u64 {
        self.requested_tick
    }
    pub fn stage(&self) -> ImpactReceiptStage {
        self.stage
    }
    /// Session-local ordinal of successful durable journal acknowledgements.
    pub fn journal_revision(&self) -> Option<u64> {
        self.journal_revision
    }
    pub fn committed_tick(&self) -> Option<u64> {
        self.committed_tick
    }
    pub fn reason(&self) -> Option<&str> {
        self.reason.as_deref()
    }
}

pub(super) struct ImpactReceiptLedger {
    rows: VecDeque<ImpactReceipt>,
    revision: u64,
}

impl Default for ImpactReceiptLedger {
    fn default() -> Self {
        Self {
            rows: VecDeque::new(),
            revision: 0,
        }
    }
}

impl ImpactReceiptLedger {
    pub(super) fn accepted(&mut self, receipt: ImpactReceipt) {
        if self.rows.len() == 64 {
            self.rows.pop_front();
        }
        self.rows.push_back(receipt);
    }

    pub(super) fn committed(&mut self, world: WorldInstanceId, id: u64, tick: u64) {
        self.revision = self.revision.saturating_add(1);
        if let Some(row) = self.rows.iter_mut().find(|row| {
            row.world == world && row.id == id && row.stage == ImpactReceiptStage::OwnerAccepted
        }) {
            row.stage = ImpactReceiptStage::OwnerCommitted;
            row.journal_revision = Some(self.revision);
            row.committed_tick = Some(tick);
        }
    }

    pub(super) fn fail_pending(&mut self, reason: &str) {
        for row in self
            .rows
            .iter_mut()
            .filter(|row| row.stage == ImpactReceiptStage::OwnerAccepted)
        {
            row.stage = ImpactReceiptStage::Failed;
            row.reason = Some(reason.chars().take(256).collect());
        }
    }

    pub(super) fn get(&self, world: WorldInstanceId, id: u64) -> Option<ImpactReceipt> {
        self.rows
            .iter()
            .find(|row| row.world == world && row.id == id)
            .cloned()
    }
}

impl Impacts {
    pub(in crate::runtime) fn record_impact_accepted(&mut self, receipt: ImpactReceipt) {
        self.receipts.accepted(receipt);
    }

    pub(in crate::runtime) fn record_impact_committed(
        &mut self,
        world: WorldInstanceId,
        id: u64,
        tick: u64,
    ) {
        self.receipts.committed(world, id, tick);
    }

    pub(in crate::runtime) fn impact_receipt(
        &self,
        world: WorldInstanceId,
        id: u64,
    ) -> Option<ImpactReceipt> {
        self.receipts.get(world, id)
    }
}
