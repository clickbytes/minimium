//! Inputs prepared by a process or embedded host before constructing the owner.
use crate::{
    authentication,
    composition::{AsyncWorkPolicy, ExcavationConfig, ServerCombatConfig, ShutdownRequest},
    executor,
    modules::ModuleRegistry,
    runtime, sector_streaming, server_command, ServerGameplayModule,
};
use minimium_diagnostics::DomainLogger;
use minimium_environment::time::{SessionNonce, WorldTimeMs};
use std::{
    io,
    sync::Arc,
    time::{Instant, SystemTime, UNIX_EPOCH},
};
mod commands;
pub use commands::*;

#[derive(Clone, Copy, Debug)]
pub struct RuntimeTime {
    pub monotonic: Instant,
    pub utc: Result<WorldTimeMs, RuntimeUtcError>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuntimeUtcError {
    BeforeEpoch,
    MillisecondOverflow,
    OutOfSupportedRange,
}
impl std::fmt::Display for RuntimeUtcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for RuntimeUtcError {}
impl RuntimeUtcError {
    pub(crate) fn environment_error(self) -> crate::environment::EnvironmentClockError {
        use crate::environment::{EnvironmentClockError, EnvironmentClockErrorKind as Kind};
        let kind = if self == Self::OutOfSupportedRange {
            Kind::TimeRange
        } else {
            Kind::SystemClock
        };
        EnvironmentClockError::caused_by(kind, "host UTC unavailable", self)
    }
}
impl RuntimeTime {
    /// Explicit OS clock adapter. A wall-clock fault travels with the monotonic
    /// sample so the existing clock fault/save-error policy remains observable.
    pub fn system_now() -> Self {
        let monotonic = Instant::now();
        let utc = (|| {
            let millis = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|_| RuntimeUtcError::BeforeEpoch)?
                .as_millis();
            let millis = i64::try_from(millis).map_err(|_| RuntimeUtcError::MillisecondOverflow)?;
            WorldTimeMs::new(millis).map_err(|_| RuntimeUtcError::OutOfSupportedRange)
        })();
        Self { monotonic, utc }
    }
}

/// Validated module/component catalog shared by storage loading and construction.
pub struct RuntimeModules {
    pub(crate) catalog: minimium_commands::operation::OperationCatalog,
    pub(crate) registry:
        ModuleRegistry<crate::commands::ModuleHandler, runtime::schedule::SystemFn>,
    pub(crate) components: minimium_world::entity::ComponentRegistry,
    pub(crate) schedule: runtime::schedule::ServerSchedule,
}
impl RuntimeModules {
    pub fn new(modules: &[Arc<ServerGameplayModule>]) -> io::Result<Self> {
        let registry = crate::commands::initialize_modules(modules).map_err(io::Error::other)?;
        // Validate the complete host graph before opening any host service, retaining
        // this same validated sequence for execution after construction.
        let definition = crate::commands::production_schedule_definition();
        let schedule =
            crate::commands::build_schedule(&registry, &definition).map_err(io::Error::other)?;
        let components = crate::commands::component_registry(&registry)
            .map_err(|error| io::Error::other(format!("invalid components: {error:?}")))?;
        let catalog = crate::server_command::catalog::build(&registry).map_err(io::Error::other)?;
        Ok(Self {
            catalog,
            registry,
            components,
            schedule,
        })
    }
    pub fn components(&self) -> &minimium_world::entity::ComponentRegistry {
        &self.components
    }
}

/// The existing bounded pools, allocated explicitly by the host, then moved to
/// the owner. Construction of `ServerRuntime` starts no thread or console reader.
pub struct RuntimeExecution {
    pub(crate) executor: executor::Executor,
    pub(crate) sectors: sector_streaming::SectorGenerationPool,
    pub(crate) console: Option<std::sync::mpsc::Receiver<server_command::ServerCommand>>,
}
impl RuntimeExecution {
    pub fn new(lanes: usize, sector_workers: usize) -> io::Result<Self> {
        if !(1..=64).contains(&lanes) || !(1..=16).contains(&sector_workers) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "runtime requires 1..64 lanes and 1..16 sector workers",
            ));
        }
        Ok(Self {
            executor: executor::Executor::new(lanes)?,
            sectors: sector_streaming::SectorGenerationPool::new(sector_workers)?,
            console: None,
        })
    }
}

pub struct RuntimeLoggers {
    pub server: DomainLogger,
    pub transport: DomainLogger,
    pub environment: DomainLogger,
    pub protocol: DomainLogger,
    pub world: DomainLogger,
    pub sectors: DomainLogger,
    pub simulation: DomainLogger,
}
pub struct RuntimeComposition {
    pub combat: ServerCombatConfig,
    pub impacts: Option<minimium_world::impact::ImpactProfile>,
    pub excavation: ExcavationConfig,
    pub async_work_policy: AsyncWorkPolicy,
    pub modules: RuntimeModules,
    pub loggers: RuntimeLoggers,
    pub authentication: authentication::SharedAuthenticationProvider,
    pub shutdown: ShutdownRequest,
    pub session_nonce: SessionNonce,
}
pub mod replies;
