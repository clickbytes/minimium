use super::ClockPropertySnapshot;
use minimium_environment::console::values::{OwnerTimeDiagnostics, TimeDiagnostics};

impl ClockPropertySnapshot {
    pub fn diagnostics(&self) -> TimeDiagnostics {
        TimeDiagnostics {
            status: self.status(),
            owner: Some(OwnerTimeDiagnostics {
                fault: self.fault,
                correction_ms: self.correction_ms,
                dirty_revision: self.dirty_revision,
            }),
        }
    }
}
