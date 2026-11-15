use super::WorldRegistry;

impl WorldRegistry {
    pub(crate) fn environment_snapshot(
        &self,
        limit: usize,
    ) -> (
        Vec<(u128, u64, minimium_environment::EnvironmentProfile)>,
        bool,
    ) {
        let mut rows = Vec::with_capacity(limit.min(self.instances.len()));
        let mut truncated = false;
        for (id, instance) in &self.instances {
            if rows.len() == limit {
                truncated = true;
                break;
            }
            rows.push((
                id.get(),
                instance.clock_id.get(),
                instance.environment_profile,
            ));
        }
        (rows, truncated)
    }
}
