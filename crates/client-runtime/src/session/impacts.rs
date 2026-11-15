use super::*;

impl ClientSession {
    pub fn impact_ready(&self) -> bool {
        self.impacts.context == self.world_context && self.impacts.ready
    }
    pub fn impact_bodies(&self) -> impl Iterator<Item = &(minimium_world::impact::BodyState, u64)> {
        self.impacts.bodies.values().filter(|_| self.impact_ready())
    }
    pub fn terrain_revision(&self) -> u64 {
        if self.impact_ready() {
            self.excavation.craters().map_or(0, |t| t.revision())
        } else {
            0
        }
    }
}
