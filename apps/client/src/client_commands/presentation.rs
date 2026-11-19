use minimium_commands::{lex_command_line_with_spans, CommandError, CommandErrorCode};

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
    ("dev goto", "dev goto LATITUDE LONGITUDE"),
    ("dev look", "dev look YAW_DEG PITCH_DEG"),
];

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum PresentationOperation {
    Developer(DeveloperOperation),
    AtmosphereStatus,
    TerrainStatus,
    WeatherStatus,
    SkyInfo,
    SkyBodies,
    MeteorStatus,
    MeteorSpawn(super::MeteorPreset, Option<u64>),
    MeteorClear,
    MeteorLight(super::MeteorLightDebug),
    MeteorDebug(super::MeteorDebug),
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum DeveloperOperation {
    Clouds,
    BolidTransmission(bool),
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
    Goto {
        latitude: f32,
        longitude: f32,
    },
    Look {
        yaw_degrees: f32,
        pitch_degrees: f32,
    },
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
pub(crate) enum PresentationCapability {
    DeveloperRenderer,
    #[cfg(feature = "desktop")]
    EarthMapTeleport,
    WeatherDiagnostics,
    SkyDiagnostics,
    MeteorDiagnostics,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PresentationRequirement {
    HostOnly,
    ReadyPresentation,
    ActiveWorldPresentation,
    ActiveEarthWorld,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
    UnsupportedHost,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PresentationUnavailable {
    pub capability: PresentationCapability,
    pub requirement: PresentationRequirement,
    pub reason: PresentationUnavailableReason,
}

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

impl PresentationRequirement {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::HostOnly => "an initialized host",
            Self::ReadyPresentation => "a ready presentation matching the current context",
            Self::ActiveWorldPresentation => "an active world with ready presentation",
            Self::ActiveEarthWorld => "an active, ready Earth world",
        }
    }
}

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
            Self::UnsupportedHost => "this host has no presentation adapter",
        }
    }
}

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

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum PresentationResult {
    #[cfg(feature = "desktop")]
    Output(Vec<String>),
    Unavailable(PresentationUnavailable),
}

impl PresentationOperation {
    pub(crate) fn capability(&self) -> PresentationCapability {
        match self {
            Self::Developer(_) => PresentationCapability::DeveloperRenderer,
            Self::AtmosphereStatus | Self::WeatherStatus | Self::TerrainStatus => {
                PresentationCapability::WeatherDiagnostics
            }
            Self::SkyInfo | Self::SkyBodies => PresentationCapability::SkyDiagnostics,
            Self::MeteorStatus
            | Self::MeteorSpawn(_, _)
            | Self::MeteorClear
            | Self::MeteorLight(_)
            | Self::MeteorDebug(_) => PresentationCapability::MeteorDiagnostics,
        }
    }

    pub(crate) fn requirement(&self) -> PresentationRequirement {
        match self {
            Self::Developer(DeveloperOperation::BolidTransmission(_))
            | Self::Developer(DeveloperOperation::Look { .. })
            | Self::AtmosphereStatus
            | Self::WeatherStatus
            | Self::TerrainStatus
            | Self::MeteorStatus
            | Self::MeteorClear
            | Self::MeteorLight(_)
            | Self::MeteorDebug(_) => PresentationRequirement::HostOnly,
            Self::Developer(DeveloperOperation::BolidStatus) => {
                PresentationRequirement::ActiveWorldPresentation
            }
            Self::Developer(DeveloperOperation::Goto { .. }) => {
                PresentationRequirement::ActiveEarthWorld
            }
            Self::Developer(DeveloperOperation::BolidReplay { .. }) => {
                PresentationRequirement::ActiveWorldPresentation
            }
            Self::Developer(DeveloperOperation::BolidPath(_))
            | Self::Developer(DeveloperOperation::BolidFragments)
            | Self::Developer(DeveloperOperation::BolidLighting) => {
                PresentationRequirement::ActiveWorldPresentation
            }
            Self::SkyInfo | Self::SkyBodies => PresentationRequirement::ReadyPresentation,
            Self::Developer(DeveloperOperation::Clouds) | Self::MeteorSpawn(_, _) => {
                PresentationRequirement::ReadyPresentation
            }
        }
    }
}

pub(super) fn parse(input: &str) -> Option<Result<PresentationOperation, CommandError>> {
    let tokens = lex_command_line_with_spans(input.trim()).ok()?;
    let words = tokens
        .into_iter()
        .map(|token| token.value)
        .collect::<Vec<_>>();
    if words.first().map(String::as_str) == Some("dev") {
        Some(parse_developer(&words))
    } else {
        None
    }
}

#[cfg(feature = "desktop")]
pub(super) fn is_developer_family(input: &str) -> bool {
    let Ok(tokens) = lex_command_line_with_spans(input.trim()) else {
        return false;
    };
    let words = tokens
        .iter()
        .map(|token| token.value.as_str())
        .collect::<Vec<_>>();
    matches!(
        words.as_slice(),
        ["dev", "clouds", ..] | ["dev", "goto", ..] | ["dev", "look", ..]
    ) || (words.first() == Some(&"dev")
        && words.get(1) == Some(&"bolid")
        && words.get(2).is_some_and(|subcommand| {
            matches!(
                *subcommand,
                "transmission" | "path" | "fragments" | "lighting" | "status" | "replay" | "freeze"
            )
        }))
}

fn parse_developer(words: &[String]) -> Result<PresentationOperation, CommandError> {
    let invalid = invalid_dev;
    let op = match words.get(1).map(String::as_str) {
        Some("clouds") if words.len() == 2 => DeveloperOperation::Clouds,
        Some("goto") if words.len() == 4 => {
            let latitude = parse_finite(&words[2]).ok_or_else(invalid)?;
            let longitude = parse_finite(&words[3]).ok_or_else(invalid)?;
            if !(-90.0..=90.0).contains(&latitude) || !(-180.0..=180.0).contains(&longitude) {
                return Err(invalid());
            }
            DeveloperOperation::Goto {
                latitude,
                longitude,
            }
        }
        Some("look") if words.len() == 4 => {
            let yaw_degrees = parse_finite(&words[2]).ok_or_else(invalid)?;
            let pitch_degrees = parse_finite(&words[3]).ok_or_else(invalid)?;
            if !(-89.0..=89.0).contains(&pitch_degrees) {
                return Err(invalid());
            }
            DeveloperOperation::Look {
                yaw_degrees,
                pitch_degrees,
            }
        }
        Some("bolid") => parse_bolid(&words[2..]).map_err(|_| invalid())?,
        _ => return Err(unknown_dev(&words.join(" "))),
    };
    Ok(PresentationOperation::Developer(op))
}

fn parse_bolid(words: &[String]) -> Result<DeveloperOperation, ()> {
    match words.first().map(String::as_str) {
        Some("transmission") if words.len() == 2 => match words[1].as_str() {
            "on" => Ok(DeveloperOperation::BolidTransmission(true)),
            "off" => Ok(DeveloperOperation::BolidTransmission(false)),
            _ => Err(()),
        },
        Some("path") if words.len() == 11 => {
            let path = BolidPath {
                id: words[1].parse().map_err(|_| ())?,
                kind: words[2].parse().map_err(|_| ())?,
                seed: words[3].parse().map_err(|_| ())?,
                duration_ms: words[4].parse().map_err(|_| ())?,
                start_world_m: [
                    parse_finite(&words[5]).ok_or(())?,
                    parse_finite(&words[6]).ok_or(())?,
                    parse_finite(&words[7]).ok_or(())?,
                ],
                end_world_m: [
                    parse_finite(&words[8]).ok_or(())?,
                    parse_finite(&words[9]).ok_or(())?,
                    parse_finite(&words[10]).ok_or(())?,
                ],
            };
            let event = minimium_protocol::BolidEvent {
                id: path.id,
                start_simulation_ms: 0,
                seed: path.seed,
                start_world_m: path.start_world_m,
                end_world_m: path.end_world_m,
                duration_ms: path.duration_ms,
                kind: path.kind,
            };
            if !event.valid() {
                return Err(());
            }
            Ok(DeveloperOperation::BolidPath(path))
        }
        Some("fragments") if words.len() == 1 => Ok(DeveloperOperation::BolidFragments),
        Some("lighting") if words.len() == 1 => Ok(DeveloperOperation::BolidLighting),
        Some("status") if words.len() == 1 => Ok(DeveloperOperation::BolidStatus),
        Some("replay" | "freeze") if (2..=4).contains(&words.len()) => {
            let age_seconds = parse_finite(&words[1]).ok_or(())?;
            if !(0.0..=120.0).contains(&age_seconds) {
                return Err(());
            }
            Ok(DeveloperOperation::BolidReplay {
                age_seconds,
                seed: words
                    .get(2)
                    .map(|value| value.parse().map_err(|_| ()))
                    .transpose()?,
                event_id: words
                    .get(3)
                    .map(|value| value.parse().map_err(|_| ()))
                    .transpose()?,
                frozen: words[0] == "freeze",
            })
        }
        _ => Err(()),
    }
}

fn parse_finite(value: &str) -> Option<f32> {
    let value = value.parse::<f32>().ok()?;
    value.is_finite().then_some(value)
}

fn invalid_dev() -> CommandError {
    CommandError::new(
        CommandErrorCode::InvalidArguments,
        Some("dev"),
        "invalid developer presentation arguments",
        Some("dev clouds | dev bolid transmission on|off | dev bolid path ID KIND SEED DURATION_MS X0 Y0 Z0 X1 Y1 Z1 | dev bolid fragments|lighting|status | dev bolid replay|freeze AGE [SEED [EVENT_ID]] | dev goto LATITUDE LONGITUDE | dev look YAW_DEG PITCH_DEG"),
    )
}

fn unknown_dev(input: &str) -> CommandError {
    CommandError::new(
        CommandErrorCode::UnknownCommand,
        Some("dev"),
        format!("unknown developer presentation command: {input}"),
        None::<String>,
    )
}
