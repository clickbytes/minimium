use super::status::{ClockSource, TimeStatus};
use minimium_commands::operation::CommandValue as V;

/// One coherent clock sample, with owner-only fields explicitly optional.
#[derive(Clone, Copy, Debug)]
pub struct TimeDiagnostics {
    pub status: TimeStatus,
    pub owner: Option<OwnerTimeDiagnostics>,
}

#[derive(Clone, Copy, Debug)]
pub struct OwnerTimeDiagnostics {
    pub fault: &'static str,
    pub correction_ms: Option<i128>,
    pub dirty_revision: Option<u64>,
}

impl TimeDiagnostics {
    pub fn value(self) -> V {
        let sample = self.status.sample;
        let nonce = sample.session_nonce();
        let mut nonce_hex = String::with_capacity(nonce.as_bytes().len() * 2);
        for byte in nonce.as_bytes() {
            use std::fmt::Write;
            let _ = write!(nonce_hex, "{byte:02x}");
        }
        V::Record(
            [
                (
                    "source".into(),
                    V::Enum(
                        match self.status.source {
                            ClockSource::OwnerSnapshot => "owner_snapshot",
                            ClockSource::SessionSample => "session_sample",
                            ClockSource::LocalDemo => "local_demo",
                        }
                        .into(),
                    ),
                ),
                ("quality".into(), V::Enum(self.status.quality.into())),
                (
                    "sample_age_ms".into(),
                    self.status.age_ms.map_or(V::Unit, V::U64),
                ),
                ("clock_id".into(), V::U64(sample.clock_id().get())),
                ("runtime_session".into(), V::Text(nonce_hex)),
                ("committed_tick".into(), V::U64(sample.boundary().tick())),
                (
                    "calendar_epoch".into(),
                    V::U64(sample.boundary().calendar_epoch().get()),
                ),
                ("clock_revision".into(), V::U64(sample.revision().get())),
                ("requested_rate_ppm".into(), V::U64(sample.rate_ppm().get())),
                ("achieved_rate_ppm".into(), V::U64(sample.achieved_ppm())),
                ("limit".into(), V::Enum(sample.limit().label().into())),
                (
                    "owner_fault".into(),
                    self.owner.map_or(V::Unit, |o| V::Enum(o.fault.into())),
                ),
                (
                    "owner_correction_ms".into(),
                    self.owner
                        .and_then(|o| o.correction_ms)
                        .map_or(V::Unit, |value| V::decimal(value, 0)),
                ),
                (
                    "owner_dirty_revision".into(),
                    self.owner
                        .and_then(|o| o.dirty_revision)
                        .map_or(V::Unit, V::U64),
                ),
            ]
            .into(),
        )
    }
}
