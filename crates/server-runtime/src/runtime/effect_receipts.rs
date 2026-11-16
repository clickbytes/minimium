use minimium_gameplay::EffectReceipt;
use minimium_world::world::WorldInstanceId;

/// Startup initialization failure with typed receipts for all accepted startup
/// effects committed or rejected before initialization stopped.
#[derive(Debug)]
pub struct StartupEffectFailure {
    world: WorldInstanceId,
    receipts: Vec<EffectReceipt>,
    rejected_pushes: usize,
}

impl StartupEffectFailure {
    pub(crate) fn new(
        world: WorldInstanceId,
        receipts: Vec<EffectReceipt>,
        rejected_pushes: usize,
    ) -> Self {
        Self {
            world,
            receipts,
            rejected_pushes,
        }
    }

    pub const fn world(&self) -> WorldInstanceId {
        self.world
    }

    pub fn receipts(&self) -> &[EffectReceipt] {
        &self.receipts
    }

    pub const fn rejected_pushes(&self) -> usize {
        self.rejected_pushes
    }
}

impl std::fmt::Display for StartupEffectFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "startup effects failed for {:?}; {} typed receipts and {} rejected pushes are available in this error",
            self.world,
            self.receipts.len(),
            self.rejected_pushes,
        )
    }
}

impl std::error::Error for StartupEffectFailure {}
