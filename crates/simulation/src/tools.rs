//! Authoritative tool admission. The host supplies position and first-hit queries.
use minimium_world::{
    items::{ActionRejection, ActionResult, Inventory, ToolKind, ToolUseIntent},
    terrain::{SurfaceHit, SurfaceTarget, TerrainMaterialSet},
};

#[derive(Clone, Copy, Debug)]
pub struct ToolDefinition {
    kind: ToolKind,
    reach_m: f64,
    radius_m: f64,
    allowed_materials: TerrainMaterialSet,
}

impl ToolDefinition {
    pub const fn new(
        kind: ToolKind,
        reach_m: f64,
        radius_m: f64,
        allowed_materials: TerrainMaterialSet,
    ) -> Self {
        Self {
            kind,
            reach_m,
            radius_m,
            allowed_materials,
        }
    }

    pub const fn kind(self) -> ToolKind {
        self.kind
    }

    pub const fn reach_m(self) -> f64 {
        self.reach_m
    }

    pub const fn radius_m(self) -> f64 {
        self.radius_m
    }

    pub const fn allowed_materials(self) -> TerrainMaterialSet {
        self.allowed_materials
    }

    pub fn valid(self) -> bool {
        (0.25..=3.0).contains(&self.reach_m)
            && (0.25..=0.6).contains(&self.radius_m)
            && self.allowed_materials.valid()
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ToolProfile {
    definitions: [ToolDefinition; 2],
    pub cooldown_ms: u64,
}

impl ToolProfile {
    pub const fn new(definitions: [ToolDefinition; 2], cooldown_ms: u64) -> Self {
        Self {
            definitions,
            cooldown_ms,
        }
    }

    pub fn valid(self) -> bool {
        self.cooldown_ms >= 500
            && self.definitions[0].valid()
            && self.definitions[1].valid()
            && self.definitions[0].kind() != self.definitions[1].kind()
            && self.definition(ToolKind::Shovel).is_some()
            && self.definition(ToolKind::Pickaxe).is_some()
    }

    pub fn definition(self, kind: ToolKind) -> Option<ToolDefinition> {
        self.definitions
            .iter()
            .copied()
            .find(|definition| definition.kind() == kind)
    }

    pub fn max_reach_m(self) -> f64 {
        self.definitions[0]
            .reach_m()
            .max(self.definitions[1].reach_m())
    }

    pub fn max_radius_m(self) -> f64 {
        self.definitions[0]
            .radius_m()
            .max(self.definitions[1].radius_m())
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
    pub definition: ToolDefinition,
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
        let definition = self
            .profile
            .definition(tool)
            .ok_or(ActionRejection::InvalidIntent)?;
        let direction = intent.direction.map(|v| v / length);
        let hit = scene
            .first_hit(direction, definition.reach_m())
            .ok_or(ActionRejection::OutOfReach)?;
        if !(0.0..=definition.reach_m()).contains(&hit.distance_m) {
            return Err(ActionRejection::OutOfReach);
        }
        let SurfaceTarget::Terrain(key) = hit.target else {
            return Err(ActionRejection::Obstructed);
        };
        if key.world != intent.world {
            return Err(ActionRejection::Stale);
        }
        if !definition.allowed_materials().contains(hit.material) {
            return Err(ActionRejection::WrongMaterial);
        }
        self.last_id = intent.action_id;
        self.last_use_ms = Some(now_ms);
        self.pending = Some(intent.action_id);
        Ok(ToolAction {
            intent,
            hit,
            definition,
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
