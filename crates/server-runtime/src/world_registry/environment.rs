use super::WorldRegistry;
use std::fmt::Write as _;

impl WorldRegistry {
    pub(crate) fn environment_summary(&self) -> String {
        let mut summary = String::new();
        for (instance_id, instance) in &self.instances {
            if !summary.is_empty() {
                summary.push(',');
            }
            let _ = write!(
                summary,
                "{}:{}:{:?}",
                instance_id,
                instance.clock_id.get(),
                instance.environment_profile
            );
        }
        summary
    }
}
