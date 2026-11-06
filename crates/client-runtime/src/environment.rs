//! Immutable environment state prepared by the client session for presentation.
pub mod diagnostics;
pub use diagnostics::{ClockDiagnostics, ClockSource, SkyDiagnosticsSnapshot};

use minimium_environment::EnvironmentDescriptor;
use minimium_protocol::WorldContext;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SynchronizationQuality {
    Initializing,
    Synchronized,
    Stale,
    Unavailable,
    LocalDemo,
}

impl SynchronizationQuality {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Initializing => "Initializing",
            Self::Synchronized => "Synchronized",
            Self::Stale => "Stale",
            Self::Unavailable => "Unavailable",
            Self::LocalDemo => "LocalDemo",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EnvironmentFrameInput {
    pub estimated_world_utc_ms: Option<i64>,
    pub render_world_utc_ms: Option<i64>,
    pub descriptor: Option<EnvironmentDescriptor>,
    pub context: Option<WorldContext>,
    pub revision: Option<u64>,
    pub rate_ppm: Option<u64>,
    pub quality: SynchronizationQuality,
    pub clock_diagnostics: Option<ClockDiagnostics>,
}

impl EnvironmentFrameInput {
    pub const fn unavailable() -> Self {
        Self {
            estimated_world_utc_ms: None,
            render_world_utc_ms: None,
            descriptor: None,
            context: None,
            revision: None,
            rate_ppm: None,
            quality: SynchronizationQuality::Unavailable,
            clock_diagnostics: None,
        }
    }

    pub fn status_signature(self) -> (SynchronizationQuality, Option<u64>, Option<u64>) {
        (self.quality, self.revision, self.rate_ppm)
    }

    pub fn status_line(self) -> String {
        let date = self
            .render_world_utc_ms
            .filter(|_| self.estimated_world_utc_ms.is_some() && self.descriptor.is_some())
            .map(format_utc)
            .unwrap_or_else(|| "UTC unavailable".to_owned());
        let rate = self
            .rate_ppm
            .map(|ppm| format!("{:.3}x", ppm as f64 / 1_000_000.0))
            .unwrap_or_else(|| "rate unavailable".to_owned());
        let revision = self
            .revision
            .map(|value| format!("rev {value}"))
            .unwrap_or_else(|| "rev unavailable".to_owned());
        format!(
            "environment {} | {date} | {rate} | {revision}",
            self.quality.label()
        )
    }

    pub fn title_suffix(self) -> String {
        let date = self
            .render_world_utc_ms
            .filter(|_| self.estimated_world_utc_ms.is_some() && self.descriptor.is_some())
            .map(format_utc)
            .unwrap_or_else(|| "UTC --".to_owned());
        format!("{} | {date}", self.quality.label())
    }

    pub fn time_status_line(self) -> String {
        diagnostics::clock_status_line(
            self.clock_diagnostics,
            self.quality,
            self.revision,
            self.render_world_utc_ms,
            self.rate_ppm,
        )
    }
}

pub(crate) fn format_utc(timestamp_ms: i64) -> String {
    let seconds = timestamp_ms.div_euclid(1_000);
    let days = seconds.div_euclid(86_400);
    let day_seconds = seconds.rem_euclid(86_400);
    let shifted = days + 719_468;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    year += if month <= 2 { 1 } else { 0 };
    let hour = day_seconds / 3_600;
    let minute = day_seconds % 3_600 / 60;
    let second = day_seconds % 60;
    format!("{year:04}-{month:02}-{day:02} {hour:02}:{minute:02}:{second:02} UTC")
}
