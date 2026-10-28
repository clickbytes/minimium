#[derive(Clone, Copy)]
pub(super) enum Scenario {
    Heartbeat,
    Movement,
    Combat,
    Chat,
    Clustered,
    Dispersed,
}

impl Scenario {
    pub(super) fn name(self) -> &'static str {
        match self {
            Self::Heartbeat => "heartbeat",
            Self::Movement => "movement",
            Self::Combat => "combat",
            Self::Chat => "chat",
            Self::Clustered => "clustered",
            Self::Dispersed => "dispersed",
        }
    }

    pub(super) fn includes_movement(self) -> bool {
        matches!(
            self,
            Self::Movement | Self::Combat | Self::Clustered | Self::Dispersed
        )
    }

    pub(super) fn includes_combat(self) -> bool {
        matches!(self, Self::Combat)
    }

    pub(super) fn includes_chat(self) -> bool {
        matches!(self, Self::Chat)
    }
}
