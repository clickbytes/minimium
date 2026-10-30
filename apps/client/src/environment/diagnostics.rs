use minimium_core::time::{ClockId, ClockMode, OfflinePolicy};

use super::SynchronizationQuality;

pub(crate) const SKY_DIAGNOSTICS_VERSION: u32 = 3;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ClockSource {
    Server,
    Demo,
}

impl ClockSource {
    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::Server => "server",
            Self::Demo => "demo",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ClockDiagnostics {
    pub(crate) source: ClockSource,
    pub(crate) clock_id: Option<ClockId>,
    pub(crate) mode: Option<ClockMode>,
    pub(crate) offline_policy: Option<OfflinePolicy>,
    pub(crate) sample_age_real_ms: Option<u64>,
    pub(crate) clock_offset_ms: Option<f64>,
    pub(crate) probe_rtt_ms: Option<u64>,
    pub(crate) samples_accepted: Option<u64>,
    pub(crate) samples_rejected: Option<u64>,
}

impl ClockDiagnostics {
    pub(crate) const fn unavailable(source: ClockSource) -> Self {
        Self {
            source,
            clock_id: None,
            mode: None,
            offline_policy: None,
            sample_age_real_ms: None,
            clock_offset_ms: None,
            probe_rtt_ms: None,
            samples_accepted: None,
            samples_rejected: None,
        }
    }
}

pub(crate) struct SkyDiagnosticsSnapshot {
    pub(crate) version: u32,
    fields: Vec<(&'static str, String)>,
}

impl SkyDiagnosticsSnapshot {
    pub(crate) fn new() -> Self {
        Self {
            version: SKY_DIAGNOSTICS_VERSION,
            fields: Vec::with_capacity(64),
        }
    }

    pub(crate) fn push(&mut self, key: &'static str, value: impl ToString) {
        self.fields.push((key, value.to_string()));
    }

    pub(crate) fn push_optional<T: ToString>(&mut self, key: &'static str, value: Option<T>) {
        self.push(
            key,
            value.map_or_else(|| "n/a".to_owned(), |value| value.to_string()),
        );
    }

    pub(crate) fn lines(self) -> impl Iterator<Item = String> {
        let mut lines = Vec::with_capacity(self.fields.len().div_ceil(2) + 1);
        lines.push(format!("sky_diagnostics_version={}", self.version));
        for chunk in self.fields.chunks(2) {
            let line = chunk
                .iter()
                .map(|(key, value)| format!("{key}={value}"))
                .collect::<Vec<_>>()
                .join(" ");
            lines.push(line);
        }
        lines.into_iter()
    }
}

pub(crate) fn clock_status_line(
    clock: Option<ClockDiagnostics>,
    quality: SynchronizationQuality,
    revision: Option<u64>,
    utc_ms: Option<i64>,
    rate_ppm: Option<u64>,
) -> String {
    let clock = clock.unwrap_or_else(|| ClockDiagnostics::unavailable(ClockSource::Server));
    format!(
        "clock_source={} clock_id={} clock_revision={} clock_mode={} world_utc={} rate_ppm={} rate={} offline_policy={} sample_age_real_ms={} clock_offset_ms={} probe_rtt_ms={} clock_samples_accepted={} clock_samples_rejected={} clock_sync_state={}",
        clock.source.label(),
        display(clock.clock_id.map(|id| id.get())),
        display(revision),
        display(clock.mode.map(clock_mode_label)),
        utc_ms.map(format_utc_millis).unwrap_or_else(|| "n/a".to_owned()),
        display(rate_ppm),
        rate_ppm.map_or_else(|| "n/a".to_owned(), |ppm| format!("{:.6}x", ppm as f64 / 1_000_000.0)),
        display(clock.offline_policy.map(offline_policy_label)),
        display(clock.sample_age_real_ms),
        clock.clock_offset_ms.map_or_else(|| "n/a".to_owned(), |value| format!("{value:.3}")),
        display(clock.probe_rtt_ms),
        display(clock.samples_accepted),
        display(clock.samples_rejected),
        quality.label(),
    )
}

pub(crate) fn format_utc_millis(timestamp_ms: i64) -> String {
    let seconds = timestamp_ms.div_euclid(1_000);
    let millis = timestamp_ms.rem_euclid(1_000);
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
    year += i64::from(month <= 2);
    let hour = day_seconds / 3_600;
    let minute = day_seconds % 3_600 / 60;
    let second = day_seconds % 60;
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}.{millis:03}Z")
}

fn display<T: ToString>(value: Option<T>) -> String {
    value.map_or_else(|| "n/a".to_owned(), |value| value.to_string())
}

fn clock_mode_label(mode: ClockMode) -> &'static str {
    match mode {
        ClockMode::RealTime => "real_time",
        ClockMode::TimeScale => "time_scale",
    }
}

fn offline_policy_label(policy: OfflinePolicy) -> &'static str {
    match policy {
        OfflinePolicy::Pause => "pause",
        OfflinePolicy::Advance => "advance",
    }
}
