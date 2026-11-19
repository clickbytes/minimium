//! Bounded, volatile owner record for a manual bolid shower.

use minimium_core::ids::PlayerId;
use minimium_protocol::WorldContext;
use minimium_world::world::WorldInstanceId;
use std::time::Instant;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ShowerState {
    Running,
    Completed,
    PartiallyCompleted,
    Cancelled,
    Failed,
}

impl ShowerState {
    pub fn label(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Completed => "completed",
            Self::PartiallyCompleted => "partially_completed",
            Self::Cancelled => "cancelled",
            Self::Failed => "failed",
        }
    }
}

#[derive(Clone, Debug)]
pub struct BolidShowerStatus {
    pub(crate) id: u64,
    pub(crate) requested: u16,
    pub(crate) event_ids: Vec<u64>,
    pub(crate) world: WorldInstanceId,
    pub(crate) observer: PlayerId,
    pub(crate) context: WorldContext,
    pub(crate) state: ShowerState,
    pub(crate) reason: Option<String>,
    pub(crate) started_tick: u64,
    pub(crate) terminal_tick: Option<u64>,
    pub(crate) terminal_at: Option<Instant>,
}

impl BolidShowerStatus {
    pub fn id(&self) -> u64 {
        self.id
    }
    pub fn requested(&self) -> u16 {
        self.requested
    }
    pub fn emitted(&self) -> usize {
        self.event_ids.len()
    }
    pub fn remaining(&self) -> usize {
        usize::from(self.requested).saturating_sub(self.emitted())
    }
    pub fn event_ids(&self) -> &[u64] {
        &self.event_ids
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
    pub fn state(&self) -> ShowerState {
        self.state
    }
    pub fn reason(&self) -> Option<&str> {
        self.reason.as_deref()
    }
    pub fn started_tick(&self) -> u64 {
        self.started_tick
    }
    pub fn terminal_tick(&self) -> Option<u64> {
        self.terminal_tick
    }

    pub(crate) fn finish(
        &mut self,
        state: ShowerState,
        reason: Option<&str>,
        tick: u64,
        now: Instant,
    ) {
        self.state = state;
        self.reason = reason.map(|text| text.chars().take(256).collect());
        self.terminal_tick = Some(tick);
        self.terminal_at = Some(now);
    }
}
