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
}

impl MeteorPreset {
    /// Stable console label and status vocabulary (desktop console output).
    #[cfg(feature = "desktop")]
    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::Faint => "faint",
            Self::Bright => "bright",
            Self::Fireball => "fireball",
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
            _ => None,
        }
    }
}

/// Typed, detailed argument error for `sky meteors spawn`.
pub(crate) fn command_error() -> CommandError {
    CommandError::new(
        CommandErrorCode::InvalidValue,
        Some("preset"),
        "preset must be faint, bright, fireball or fragmenting",
        Some("sky meteors spawn faint|bright|fireball|fragmenting"),
    )
}

/// Session-only rendering diagnostic; never changes the event clock.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MeteorLightDebug {
    On,
    Off,
    Transmission,
}
