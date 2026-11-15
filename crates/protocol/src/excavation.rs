//! Contextual excavation inputs and bounded chunk replication records.

#[derive(Debug, Clone, PartialEq)]
pub enum ExcavationInput {
    GrantTools,
    Equip { slot: Option<u8> },
    Use { action_id: u64, direction: [f32; 3] },
    Resync { coordinates: [i32; 3] },
}

#[derive(Debug, Clone, PartialEq)]
pub enum ExcavationMessage {
    Inventory {
        inventory: minimium_world::items::Inventory,
    },
    ActionResult {
        result: minimium_world::items::ActionResult,
    },
    /// Brackets a complete, atomically-installed set of chunk images.
    BatchBegin {
        sync_id: u64,
        base_revision: u64,
        revision: u64,
        chunks: u16,
        craters: u16,
        planet_radius_m: f64,
        full: bool,
    },
    ChunkBegin {
        sync_id: u64,
        coordinates: [i32; 3],
        revision: u64,
        total_bytes: u32,
    },
    ChunkPart {
        sync_id: u64,
        coordinates: [i32; 3],
        revision: u64,
        offset: u32,
        bytes: Vec<u8>,
    },
    ChunkEnd {
        sync_id: u64,
        coordinates: [i32; 3],
        revision: u64,
    },
    BatchEnd {
        sync_id: u64,
        revision: u64,
    },
    Crater {
        sync_id: u64,
        planet_radius_m: f64,
        stamp: minimium_world::terrain_edits::CraterStamp,
    },
    FluidBegin {
        sync_id: u64,
        revision: u64,
        total_bytes: u32,
        region_min: [i32; 3],
        region_max: [i32; 3],
    },
    FluidPart {
        sync_id: u64,
        revision: u64,
        offset: u32,
        bytes: Vec<u8>,
    },
    FluidEnd {
        sync_id: u64,
        revision: u64,
    },
}
