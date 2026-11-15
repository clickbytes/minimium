//! Neutral inventory records. Items in a backpack are not world entities.
use crate::{terrain::TerrainMaterial, world::WorldInstanceId};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ToolKind {
    Shovel,
    Pickaxe,
}

impl ToolKind {
    pub fn removes(self, material: TerrainMaterial) -> bool {
        matches!(material, TerrainMaterial::Soil)
            || self == Self::Pickaxe && material == TerrainMaterial::Rock
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ItemRecord {
    pub kind: ToolKind,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Inventory {
    pub slots: [Option<ItemRecord>; 2],
    pub equipped: Option<u8>,
    pub revision: u64,
}

impl Inventory {
    /// Idempotent developer grant. The host checks developer permission first.
    pub fn ensure_tools(&mut self) -> Result<bool, ActionRejection> {
        let slots = [
            Some(ItemRecord {
                kind: ToolKind::Shovel,
            }),
            Some(ItemRecord {
                kind: ToolKind::Pickaxe,
            }),
        ];
        if self.slots == slots {
            return Ok(false);
        }
        let revision = self
            .revision
            .checked_add(1)
            .ok_or(ActionRejection::Capacity)?;
        self.slots = slots;
        self.revision = revision;
        Ok(true)
    }

    pub fn equip(&mut self, slot: Option<u8>) -> Result<(), ActionRejection> {
        if slot.is_some_and(|i| self.slots.get(usize::from(i)).is_none_or(Option::is_none)) {
            return Err(ActionRejection::NotEquipped);
        }
        if self.equipped != slot {
            self.revision = self
                .revision
                .checked_add(1)
                .ok_or(ActionRejection::Capacity)?;
            self.equipped = slot;
        }
        Ok(())
    }

    pub fn tool(self) -> Option<ToolKind> {
        self.slots
            .get(usize::from(self.equipped?))?
            .map(|item| item.kind)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ToolUseIntent {
    pub world: WorldInstanceId,
    pub action_id: u64,
    pub direction: [f64; 3],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ActionRejection {
    NotEquipped,
    InvalidIntent,
    OutOfReach,
    Obstructed,
    UnsafeGeometry,
    WrongMaterial,
    Busy,
    RateLimited,
    Stale,
    Capacity,
    Storage,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ActionResult {
    Committed {
        action_id: u64,
        mutation_id: u64,
    },
    Rejected {
        action_id: u64,
        reason: ActionRejection,
    },
}
