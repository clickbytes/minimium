//! Authoritative tool admission. The host supplies position and first-hit queries.
use minimium_world::{
    items::{ActionRejection, ActionResult, Inventory, ToolUseIntent},
    terrain::{SurfaceHit, SurfaceTarget},
};

#[derive(Clone, Copy, Debug)]
pub struct ToolProfile {
    pub reach_m: f64,
    pub radius_m: f64,
    pub cooldown_ms: u64,
}

impl ToolProfile {
    pub fn valid(self) -> bool {
        (0.25..=3.0).contains(&self.reach_m)
            && (0.25..=0.6).contains(&self.radius_m)
            && self.cooldown_ms >= 500
    }
}

/// The query must include terrain, static geometry and entities, in distance order.
pub trait ToolScene {
    fn first_hit(&self, direction: [f64; 3], reach_m: f64) -> Option<SurfaceHit>;
}

#[derive(Clone, Copy, Debug)]
pub struct ToolAction {
    pub intent: ToolUseIntent,
    pub hit: SurfaceHit,
    pub tool: minimium_world::items::ToolKind,
    pub radius_m: f64,
}

#[derive(Debug)]
pub struct ToolUseGate {
    profile: ToolProfile,
    last_id: u64,
    last_use_ms: Option<u64>,
    pending: Option<u64>,
    completed: Option<ActionResult>,
}

impl ToolUseGate {
    pub fn new(profile: ToolProfile) -> Option<Self> {
        profile.valid().then_some(Self {
            profile,
            last_id: 0,
            last_use_ms: None,
            pending: None,
            completed: None,
        })
    }

    pub fn completed(&self, id: u64) -> Option<ActionResult> {
        self.completed.filter(|result| match result {
            ActionResult::Committed { action_id, .. }
            | ActionResult::Rejected { action_id, .. } => *action_id == id,
        })
    }

    pub fn prepare(
        &mut self,
        intent: ToolUseIntent,
        inventory: Inventory,
        scene: &dyn ToolScene,
        now_ms: u64,
    ) -> Result<ToolAction, ActionRejection> {
        if self.pending.is_some() {
            return Err(ActionRejection::Busy);
        }
        if intent.action_id <= self.last_id {
            return Err(ActionRejection::Stale);
        }
        let length = intent.direction.iter().map(|v| v * v).sum::<f64>().sqrt();
        if !length.is_finite() || length < 1.0e-9 {
            return Err(ActionRejection::InvalidIntent);
        }
        if self
            .last_use_ms
            .is_some_and(|last| now_ms.saturating_sub(last) < self.profile.cooldown_ms)
        {
            return Err(ActionRejection::RateLimited);
        }
        let tool = inventory.tool().ok_or(ActionRejection::NotEquipped)?;
        let direction = intent.direction.map(|v| v / length);
        let hit = scene
            .first_hit(direction, self.profile.reach_m)
            .ok_or(ActionRejection::OutOfReach)?;
        if !(0.0..=self.profile.reach_m).contains(&hit.distance_m) {
            return Err(ActionRejection::OutOfReach);
        }
        let SurfaceTarget::Terrain(key) = hit.target else {
            return Err(ActionRejection::Obstructed);
        };
        if key.world != intent.world {
            return Err(ActionRejection::Stale);
        }
        if !tool.removes(hit.material) {
            return Err(ActionRejection::WrongMaterial);
        }
        self.last_id = intent.action_id;
        self.last_use_ms = Some(now_ms);
        self.pending = Some(intent.action_id);
        Ok(ToolAction {
            intent,
            hit,
            tool,
            radius_m: self.profile.radius_m,
        })
    }

    pub fn finish(&mut self, result: ActionResult) -> bool {
        let action = match result {
            ActionResult::Committed { action_id, .. }
            | ActionResult::Rejected { action_id, .. } => action_id,
        };
        if self.pending != Some(action) {
            return false;
        }
        self.pending = None;
        self.completed = Some(result);
        true
    }
}
