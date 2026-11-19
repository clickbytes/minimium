use fluent_bundle::FluentArgs;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolNotice {
    Shovel,
    Pickaxe,
    Stowed,
    Soil,
    Rock,
    Empty,
    Obstacle,
    Ready,
    Synchronizing,
    NoTarget,
    WrongMaterial,
    NotEquipped,
    Obstructed,
    UnsafeGeometry,
    Paused,
    Busy,
    RateLimited,
    Stale,
    Capacity,
    Storage,
    InvalidIntent,
    Pending,
    Committed,
    Controls,
    Preview,
}

impl ToolNotice {
    fn key(self) -> &'static str {
        match self {
            Self::Shovel => "tool-shovel",
            Self::Pickaxe => "tool-pickaxe",
            Self::Stowed => "tool-stowed",
            Self::Soil => "tool-soil",
            Self::Rock => "tool-rock",
            Self::Empty => "tool-empty",
            Self::Obstacle => "tool-obstacle",
            Self::Ready => "tool-ready",
            Self::Synchronizing => "tool-synchronizing",
            Self::NoTarget => "tool-no-target",
            Self::WrongMaterial => "tool-wrong-material",
            Self::NotEquipped => "tool-not-equipped",
            Self::Obstructed => "tool-obstructed",
            Self::Paused => "tool-paused",
            Self::UnsafeGeometry => "tool-unsafe-geometry",
            Self::Busy => "tool-busy",
            Self::RateLimited => "tool-rate-limited",
            Self::Stale => "tool-stale",
            Self::Capacity => "tool-capacity",
            Self::Storage => "tool-storage",
            Self::InvalidIntent => "tool-invalid-intent",
            Self::Pending => "tool-pending",
            Self::Committed => "tool-committed",
            Self::Controls => "tool-controls",
            Self::Preview => "tool-preview",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum ToolMessage {
    Notice(ToolNotice),
    Heading { tool: String },
    Target { material: String, distance: String },
    Result { action: u64, outcome: String },
}

impl ToolMessage {
    pub(super) fn key(&self) -> &'static str {
        match self {
            Self::Notice(notice) => notice.key(),
            Self::Heading { .. } => "tool-heading",
            Self::Target { .. } => "tool-target",
            Self::Result { .. } => "tool-result",
        }
    }

    pub(super) fn args(&self) -> FluentArgs<'static> {
        let mut args = FluentArgs::new();
        match self {
            Self::Notice(_) => {}
            Self::Heading { tool } => args.set("tool", tool.clone()),
            Self::Target { material, distance } => {
                args.set("material", material.clone());
                args.set("distance", distance.clone());
            }
            Self::Result { action, outcome } => {
                args.set("action", action.to_string());
                args.set("outcome", outcome.clone());
            }
        }
        args
    }
}
