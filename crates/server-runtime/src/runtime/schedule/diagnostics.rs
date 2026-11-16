#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct ScheduleDiagnostics {
    pub(crate) world_instance_count: usize,
    pub(crate) startup_system_runs: usize,
    pub(crate) expired_effect_receipts: u64,
    pub(crate) effect_backpressure_rejections: u64,
}

pub(super) const MAX_STARTUP_EFFECT_RECEIPTS: usize = 4096;
pub(super) const MAX_SYSTEM_EFFECT_RECEIPTS: usize = 4096;
