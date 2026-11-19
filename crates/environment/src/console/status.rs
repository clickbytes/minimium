use crate::time::{ClockMode, ClockSample, OfflinePolicy};
use minimium_commands::operation::CommandValue as Value;

/// Observed progress may fall outside the range accepted by the scale setter.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MeasuredRatePpm(pub u64);

#[derive(Clone, Copy, Debug)]
pub struct TimeStatus {
    pub sample: ClockSample,
    pub source: ClockSource,
    pub quality: &'static str,
    pub age_ms: Option<u64>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClockSource {
    OwnerSnapshot,
    SessionSample,
    LocalDemo,
}

impl TimeStatus {
    pub fn achieved(self) -> MeasuredRatePpm {
        MeasuredRatePpm(self.sample.achieved_ppm())
    }

    pub fn property(self, path: &str) -> Option<Value> {
        let sample = self.sample;
        Some(match path {
            "world.time.mode" => Value::Enum(
                match sample.mode() {
                    ClockMode::TimeScale => "time_scale",
                    ClockMode::RealTime => "real_time",
                }
                .into(),
            ),
            "world.time.scale" => Value::Decimal {
                coefficient: i128::from(sample.rate_ppm().get()),
                fractional_digits: 6,
            },
            "world.time.offline_policy" => Value::Enum(
                match sample.offline_policy() {
                    OfflinePolicy::Advance => "advance",
                    OfflinePolicy::Pause => "pause",
                }
                .into(),
            ),
            "world.time.utc" => Value::InstantMs(sample.world_utc_ms().get()),
            "world.time.revision" => Value::U64(sample.revision().get()),
            "world.time.achieved_scale" => Value::U64(self.achieved().0),
            _ => return None,
        })
    }

    pub fn value(self) -> Value {
        let sample = self.sample;
        let configured = sample.rate_ppm().get();
        Value::Record(
            [
                ("source".into(), Value::Enum(format!("{:?}", self.source))),
                ("quality".into(), Value::Enum(self.quality.into())),
                (
                    "sample_age_ms".into(),
                    self.age_ms.map_or(Value::Unit, Value::U64),
                ),
                ("clock_id".into(), Value::U64(sample.clock_id().get())),
                ("revision".into(), Value::U64(sample.revision().get())),
                ("tick".into(), Value::U64(sample.boundary().tick())),
                (
                    "calendar_epoch".into(),
                    Value::U64(sample.boundary().calendar_epoch().get()),
                ),
                ("utc".into(), Value::InstantMs(sample.world_utc_ms().get())),
                (
                    "mode".into(),
                    self.property("world.time.mode").expect("clock mode"),
                ),
                (
                    "offline_policy".into(),
                    self.property("world.time.offline_policy")
                        .expect("clock policy"),
                ),
                ("requested_ppm".into(), Value::U64(configured)),
                ("effective_ppm".into(), Value::U64(configured)),
                ("achieved_ppm".into(), Value::U64(self.achieved().0)),
                ("limit".into(), Value::Enum(sample.limit().label().into())),
            ]
            .into(),
        )
    }
}
