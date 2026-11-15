use super::{ClientTrafficSnapshot, RuntimeStatusSnapshot, ScheduleDiagnostics};
use crate::{
    environment::EnvironmentClock,
    metrics::RuntimeMetrics,
    storage::{PersistenceWorker, PlayerStore},
    world_registry::WorldRegistry,
};
use minimium_environment::time::MonotonicMs;

/// Private owner-only inputs for commands that operate storage or server services.
pub(crate) struct OwnerCommandContext<'a> {
    pub(crate) worlds: &'a WorldRegistry,
    pub(crate) player_store: &'a mut PlayerStore,
    pub(crate) persistence: &'a mut PersistenceWorker,
    pub(crate) map_fingerprint: u64,
    pub(crate) lanes: usize,
    pub(crate) metrics: &'a RuntimeMetrics,
    pub(crate) environment_clock: &'a mut EnvironmentClock,
    pub(crate) monotonic_ms: MonotonicMs,
    pub(crate) schedule_diagnostics: ScheduleDiagnostics,
    pub(crate) runtime_status: Option<RuntimeStatusSnapshot>,
    pub(crate) client_traffic: ClientTrafficSnapshot,
}
