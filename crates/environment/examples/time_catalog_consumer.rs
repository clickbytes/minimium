//! External CPU consumer of the domain-owned clock catalog and exact values.
use minimium_commands::operation::{LegacyPackage, OperationKind};
use minimium_commands::{PropertyLifecycle, PropertyPermission, PropertySpec, PropertyType};
use minimium_environment::console::{
    self,
    status::{ClockSource, TimeStatus},
    ClockReadBinding,
};
use minimium_environment::time::{
    ClockId, ClockMode, ClockRevision, ClockSample, MonotonicMs, OfflinePolicy, SessionNonce,
    TimeScalePpm, WorldTimeMs,
};
use minimium_environment::{MeasuredRatePpm, TimeDiagnostics};

static ACHIEVED: PropertySpec = PropertySpec::new(
    "world.time.achieved_scale",
    PropertyType::Integer,
    Some("ppm"),
    "Exact measured server progress",
    None,
    None,
    "EnvironmentClock",
    PropertyPermission::Local,
    PropertyLifecycle::ReadOnly,
    false,
    false,
);

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut package = LegacyPackage::default();
    package.property(
        console::clock_entry(ACHIEVED.key, ClockReadBinding::SessionSample)
            .expect("domain property"),
        &ACHIEVED,
    )?;
    console::register_operations(&mut package, ClockReadBinding::SessionSample)?;
    let catalog = package.finish()?;
    assert_eq!(
        catalog
            .resolve("world.time.achieved_scale")
            .unwrap()
            .spec
            .kind,
        OperationKind::Property
    );
    assert_eq!(
        catalog.resolve("world.time.diagnostics").unwrap().spec.kind,
        OperationKind::Method
    );
    let sample = ClockSample::new(
        ClockId::MVP,
        ClockRevision::INITIAL,
        SessionNonce::new([1; minimium_environment::time::SESSION_NONCE_BYTES]),
        MonotonicMs::new(7),
        WorldTimeMs::new(0)?,
        ClockMode::TimeScale,
        TimeScalePpm::ONE_X,
        OfflinePolicy::Pause,
    )?
    .with_progress(u64::MAX, minimium_environment::time::SimulationLimit::None);
    let status = TimeStatus {
        sample,
        source: ClockSource::SessionSample,
        quality: "Synchronized",
        age_ms: Some(2),
    };
    assert_eq!(status.achieved(), MeasuredRatePpm(u64::MAX));
    let report = TimeDiagnostics {
        status,
        owner: None,
    }
    .value();
    println!("{}", report.encode());
    Ok(())
}
