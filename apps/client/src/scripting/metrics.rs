use minimium_commands::operation::CommandValue as V;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub(crate) struct WaitClock {
    ledger: Arc<Mutex<super::Ledger>>,
    started: Instant,
}

impl WaitClock {
    pub(crate) fn start(ledger: &Arc<Mutex<super::Ledger>>) -> Self {
        Self {
            ledger: Arc::clone(ledger),
            started: Instant::now(),
        }
    }
}

impl Drop for WaitClock {
    fn drop(&mut self) {
        if let Ok(mut ledger) = self.ledger.lock() {
            ledger.metrics.add_wait(self.started.elapsed());
        }
    }
}

#[derive(Default)]
pub(crate) struct Metrics {
    pub(crate) load: Option<Duration>,
    pub(crate) worker_start: Option<Duration>,
    pub(crate) compile: Option<Duration>,
    pub(crate) cache_lookup: Option<Duration>,
    pub(crate) adapter_setup: Option<Duration>,
    pub(crate) evaluate: Option<Duration>,
    pub(crate) wait_for_owner: Option<Duration>,
    pub(crate) format: Option<Duration>,
    pub(crate) source_bytes: Option<usize>,
    pub(crate) result_bytes: Option<usize>,
    pub(crate) cache_hit: Option<bool>,
    pub(crate) cache_entries: Option<usize>,
    pub(crate) cached_source_bytes: Option<usize>,
    pub(crate) termination_code: Option<String>,
}

impl Metrics {
    pub(crate) fn add_wait(&mut self, elapsed: Duration) {
        self.wait_for_owner = Some(
            self.wait_for_owner
                .unwrap_or_default()
                .saturating_add(elapsed),
        );
    }

    pub(crate) fn report(&self) -> V {
        let mut values = BTreeMap::new();
        for (name, value) in [
            ("load", self.load),
            ("worker_start", self.worker_start),
            ("compile", self.compile),
            ("cache_lookup", self.cache_lookup),
            ("adapter_setup", self.adapter_setup),
            ("evaluate", self.evaluate),
            ("wait_for_owner", self.wait_for_owner),
            ("format", self.format),
        ] {
            values.insert(name.into(), phase(value));
        }
        values.insert("source_bytes".into(), optional_usize(self.source_bytes));
        values.insert("result_bytes".into(), optional_usize(self.result_bytes));
        values.insert("cache_hit".into(), self.cache_hit.map_or(V::Unit, V::Bool));
        values.insert("cache_entries".into(), optional_usize(self.cache_entries));
        values.insert(
            "cached_source_bytes".into(),
            optional_usize(self.cached_source_bytes),
        );
        values.insert(
            "termination_code".into(),
            self.termination_code
                .as_ref()
                .map_or(V::Unit, |code| V::Enum(code.clone())),
        );
        V::Record(values)
    }
}

fn optional_usize(value: Option<usize>) -> V {
    value.map_or(V::Unit, |n| V::U64(n as u64))
}

fn phase(value: Option<Duration>) -> V {
    V::Record(BTreeMap::from([
        (
            "state".into(),
            V::Enum(
                if value.is_some() {
                    "measured"
                } else {
                    "not_measured"
                }
                .into(),
            ),
        ),
        (
            "source".into(),
            V::Enum(
                if value.is_some() {
                    "monotonic_real_time"
                } else {
                    "unavailable"
                }
                .into(),
            ),
        ),
        (
            "duration_us".into(),
            value.map_or(V::Unit, |d| {
                V::U64(d.as_micros().min(u128::from(u64::MAX)) as u64)
            }),
        ),
    ]))
}
