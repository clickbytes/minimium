use minimium_commands::{CommandError, CommandErrorCode};

pub(super) const COMMAND_FORMS: &[(&str, &str)] = &[
    ("dev clouds", "dev clouds"),
    ("dev bolid transmission", "dev bolid transmission on|off"),
    (
        "dev bolid path",
        "dev bolid path ID KIND SEED DURATION_MS X0 Y0 Z0 X1 Y1 Z1",
    ),
    ("dev bolid fragments", "dev bolid fragments"),
    ("dev bolid lighting", "dev bolid lighting"),
    ("dev bolid status", "dev bolid status"),
    ("dev bolid replay", "dev bolid replay AGE [SEED [EVENT_ID]]"),
    ("dev bolid freeze", "dev bolid freeze AGE [SEED [EVENT_ID]]"),
    ("dev look", "dev look YAW_DEG PITCH_DEG"),
];

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum PresentationOperation {
    Developer(DeveloperOperation),
    AtmosphereStatus,
    TerrainStatus,
    WaterStatus,
    WeatherStatus,
    SkyInfo,
    SkyBodies,
    MeteorStatus,
    MeteorDebugRead,
    MeteorLightRead,
    MeteorSpawn(super::MeteorPreset, Option<u64>),
    MeteorClear,
    MeteorLight(super::MeteorLightDebug),
    MeteorDebug(super::MeteorDebug),
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum DeveloperOperation {
    Clouds,
    BolidTransmission(bool),
    BolidTransmissionRead,
    BolidPath(BolidPath),
    BolidFragments,
    BolidLighting,
    BolidReplay {
        age_seconds: f32,
        seed: Option<u64>,
        event_id: Option<u64>,
        frozen: bool,
    },
    BolidStatus,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct BolidPath {
    pub id: u64,
    pub kind: u8,
    pub seed: u64,
    pub duration_ms: u16,
    pub start_world_m: [f32; 3],
    pub end_world_m: [f32; 3],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg(feature = "desktop")]
pub(crate) enum PresentationCapability {
    DeveloperRenderer,
    #[cfg(feature = "desktop")]
    EarthMapTeleport,
    WeatherDiagnostics,
    SkyDiagnostics,
    MeteorDiagnostics,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg(feature = "desktop")]
pub(crate) enum PresentationRequirement {
    ReadyPresentation,
    ActiveWorldPresentation,
    #[cfg(feature = "desktop")]
    ActiveEarthWorld,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg(feature = "desktop")]
pub(crate) enum PresentationUnavailableReason {
    #[cfg(feature = "desktop")]
    MissingSession,
    #[cfg(feature = "desktop")]
    NoActiveWorld,
    #[cfg(feature = "desktop")]
    CpuReadinessPending,
    #[cfg(feature = "desktop")]
    PresentationPending,
    #[cfg(feature = "desktop")]
    WorldTransferPending,
    #[cfg(feature = "desktop")]
    WrongWorld,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg(feature = "desktop")]
pub(crate) struct PresentationUnavailable {
    pub capability: PresentationCapability,
    pub requirement: PresentationRequirement,
    pub reason: PresentationUnavailableReason,
}

#[cfg(feature = "desktop")]
impl PresentationCapability {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::DeveloperRenderer => "developer renderer",
            #[cfg(feature = "desktop")]
            Self::EarthMapTeleport => "Earth map teleport",
            Self::WeatherDiagnostics => "weather diagnostics",
            Self::SkyDiagnostics => "sky diagnostics",
            Self::MeteorDiagnostics => "meteor diagnostics",
        }
    }
}

#[cfg(feature = "desktop")]
impl PresentationRequirement {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::ReadyPresentation => "a ready presentation matching the current context",
            Self::ActiveWorldPresentation => "an active world with ready presentation",
            #[cfg(feature = "desktop")]
            Self::ActiveEarthWorld => "an active, ready Earth world",
        }
    }
}

#[cfg(feature = "desktop")]
impl PresentationUnavailableReason {
    pub(crate) fn label(self) -> &'static str {
        match self {
            #[cfg(feature = "desktop")]
            Self::MissingSession => "there is no connected session",
            #[cfg(feature = "desktop")]
            Self::NoActiveWorld => "there is no active world",
            #[cfg(feature = "desktop")]
            Self::CpuReadinessPending => "world content is still loading",
            #[cfg(feature = "desktop")]
            Self::PresentationPending => "the scene is not ready",
            #[cfg(feature = "desktop")]
            Self::WorldTransferPending => "a world transfer or teleport is pending",
            #[cfg(feature = "desktop")]
            Self::WrongWorld => "the active world is not Earth",
        }
    }
}

#[cfg(feature = "desktop")]
impl PresentationUnavailable {
    pub(crate) fn message(self) -> String {
        format!(
            "unavailable: {} requires {}; {}",
            self.capability.label(),
            self.requirement.label(),
            self.reason.label()
        )
    }
}

#[cfg(feature = "desktop")]
impl PresentationOperation {
    pub(crate) fn capability(&self) -> PresentationCapability {
        match self {
            Self::Developer(_) => PresentationCapability::DeveloperRenderer,
            Self::AtmosphereStatus
            | Self::WeatherStatus
            | Self::TerrainStatus
            | Self::WaterStatus => PresentationCapability::WeatherDiagnostics,
            Self::SkyInfo | Self::SkyBodies => PresentationCapability::SkyDiagnostics,
            Self::MeteorStatus
            | Self::MeteorDebugRead
            | Self::MeteorLightRead
            | Self::MeteorSpawn(_, _)
            | Self::MeteorClear
            | Self::MeteorLight(_)
            | Self::MeteorDebug(_) => PresentationCapability::MeteorDiagnostics,
        }
    }
}

pub(super) fn unknown_dev(input: &str) -> CommandError {
    CommandError::new(
        CommandErrorCode::UnknownCommand,
        Some("dev"),
        format!("unknown developer presentation command: {input}"),
        None::<String>,
    )
}
