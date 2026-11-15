//! `sky meteors ...` command vocabulary: manual preset names with their typed
//! argument error. The command layer owns the vocabulary; visual parameters
//! stay in the desktop runtime (`crate::renderer::meteors`), so terminal
//! builds parse these commands without the desktop renderer modules
//! (E047-02).

use super::{CommandError, CommandErrorCode};

/// Manual ambient-meteor preset names behind `sky meteors spawn`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MeteorPreset {
    /// Dim, small, short diagnostic streak.
    Faint,
    /// Bright streak with its own transient light.
    Bright,
    /// Bolide-scale fireball with a strong transient light.
    Fireball,
    Fragmenting,
    Ablation,
    Burnout,
    Erosion,
    Droplets,
    Irregular,
    SingleSplit,
    AsymmetricSplit,
    MultipleSplit,
    Cascade,
    Dustball,
    HardCore,
    SingleFlare,
    MultipleFlares,
    TerminalFlare,
    Airburst,
    Remnants,
    DarkFlight,
    Debris,
    LowFlare,
    Grazing,
}

impl MeteorPreset {
    /// Stable console label and status vocabulary (desktop console output).
    #[cfg(feature = "desktop")]
    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::Faint => "faint",
            Self::Bright => "bright",
            Self::Fireball => "fireball",
            Self::Ablation => "ablation",
            Self::Burnout => "burnout",
            Self::Erosion => "erosion",
            Self::Droplets => "droplets",
            Self::Irregular => "irregular",
            Self::SingleSplit => "single_split",
            Self::AsymmetricSplit => "asymmetric_split",
            Self::MultipleSplit => "multiple_split",
            Self::Cascade => "cascade",
            Self::Dustball => "dustball",
            Self::HardCore => "hard_core",
            Self::SingleFlare => "single_flare",
            Self::MultipleFlares => "multiple_flares",
            Self::TerminalFlare => "terminal_flare",
            Self::Airburst => "airburst",
            Self::Remnants => "remnants",
            Self::DarkFlight => "dark_flight",
            Self::Debris => "debris",
            Self::LowFlare => "low_flare",
            Self::Grazing => "grazing",
            Self::Fragmenting => "fragmenting",
        }
    }

    /// Parses a preset argument; everything else is a detailed arg error.
    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value {
            "faint" => Some(Self::Faint),
            "bright" => Some(Self::Bright),
            "fireball" => Some(Self::Fireball),
            "fragmenting" => Some(Self::Fragmenting),
            "ablation" => Some(Self::Ablation),
            "burnout" => Some(Self::Burnout),
            "erosion" => Some(Self::Erosion),
            "droplets" => Some(Self::Droplets),
            "irregular" => Some(Self::Irregular),
            "single_split" => Some(Self::SingleSplit),
            "asymmetric_split" => Some(Self::AsymmetricSplit),
            "multiple_split" => Some(Self::MultipleSplit),
            "cascade" => Some(Self::Cascade),
            "dustball" => Some(Self::Dustball),
            "hard_core" => Some(Self::HardCore),
            "single_flare" => Some(Self::SingleFlare),
            "multiple_flares" => Some(Self::MultipleFlares),
            "terminal_flare" => Some(Self::TerminalFlare),
            "airburst" => Some(Self::Airburst),
            "remnants" => Some(Self::Remnants),
            "dark_flight" => Some(Self::DarkFlight),
            "debris" => Some(Self::Debris),
            "low_flare" => Some(Self::LowFlare),
            "grazing" => Some(Self::Grazing),
            _ => None,
        }
    }
}

/// Typed, detailed argument error for `sky meteors spawn`.
pub(crate) fn command_error() -> CommandError {
    CommandError::new(
        CommandErrorCode::InvalidValue,
        Some("preset"),
        "unknown preset or invalid seed; see help sky meteors spawn",
        Some("sky meteors spawn PRESET [decimal u64 seed]"),
    )
}

/// Session-only rendering diagnostic; never changes the event clock.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MeteorLightDebug {
    On,
    Off,
    Transmission,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MeteorDebug {
    Off,
    Light,
    Occlusion,
    Paths,
    Budget,
}

impl MeteorDebug {
    pub(crate) fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "off" => Self::Off,
            "light" => Self::Light,
            "occlusion" => Self::Occlusion,
            "paths" => Self::Paths,
            "budget" => Self::Budget,
            _ => return None,
        })
    }
}

pub(crate) const PRESET_NAMES: &[&str] = &[
    "faint",
    "bright",
    "fireball",
    "fragmenting",
    "ablation",
    "burnout",
    "erosion",
    "droplets",
    "irregular",
    "single_split",
    "asymmetric_split",
    "multiple_split",
    "cascade",
    "dustball",
    "hard_core",
    "single_flare",
    "multiple_flares",
    "terminal_flare",
    "airburst",
    "remnants",
    "dark_flight",
    "debris",
    "low_flare",
    "grazing",
];
