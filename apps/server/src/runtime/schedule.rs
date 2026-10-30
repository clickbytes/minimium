use crate::world_registry::WorldRegistry;
use minimium_platform::modules::{RegisteredSystem, Stage};

pub(crate) type SystemFn = for<'a> fn(&mut ServerSystemContext<'a>);

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct ScheduleDiagnostics {
    pub(crate) world_instance_count: usize,
    pub(crate) startup_system_runs: usize,
}

/// Systems receive only a read-only registry view and a schedule-owned
/// diagnostic sink. FixedUpdate, Environment and RenderPreparation do not
/// grant access to simulation mutation, persistence, I/O or worker pools.
pub(crate) struct ServerSystemContext<'a> {
    pub(crate) stage: Stage,
    pub(crate) worlds: &'a WorldRegistry,
    pub(crate) diagnostics: &'a mut ScheduleDiagnostics,
}

struct ScheduledSystem {
    stage: Stage,
    handler: SystemFn,
}

pub(crate) struct ServerSchedule {
    systems: Vec<ScheduledSystem>,
    startup_ran: bool,
    diagnostics: ScheduleDiagnostics,
}

impl ServerSchedule {
    pub(crate) fn from_systems(systems: &[RegisteredSystem<SystemFn>]) -> Self {
        Self {
            systems: systems
                .iter()
                .map(|system| ScheduledSystem {
                    stage: system.definition.stage,
                    handler: system.definition.handler,
                })
                .collect(),
            startup_ran: false,
            diagnostics: ScheduleDiagnostics::default(),
        }
    }

    pub(crate) fn run_stage(&mut self, stage: Stage, worlds: &WorldRegistry) {
        if stage == Stage::Startup {
            if self.startup_ran {
                return;
            }
            self.startup_ran = true;
        }
        let mut context = ServerSystemContext {
            stage,
            worlds,
            diagnostics: &mut self.diagnostics,
        };
        for system in self.systems.iter().filter(|system| system.stage == stage) {
            (system.handler)(&mut context);
        }
    }

    pub(crate) const fn diagnostics(&self) -> ScheduleDiagnostics {
        self.diagnostics
    }
}
