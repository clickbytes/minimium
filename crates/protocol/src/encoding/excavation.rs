use super::super::{
    limits::*,
    models::{ExcavationMessage, ProtocolError},
};
use super::common::write_world_context;

pub(super) fn encode(
    context: crate::WorldContext,
    message: &ExcavationMessage,
    frame: &mut Vec<u8>,
) -> Result<(), ProtocolError> {
    use minimium_world::items::{ActionRejection, ActionResult, ToolKind};

    write_world_context(context, frame);
    match message {
        ExcavationMessage::Inventory { inventory } => {
            if inventory
                .equipped
                .is_some_and(|slot| slot >= 2 || inventory.slots[usize::from(slot)].is_none())
            {
                return Err(ProtocolError::InvalidPayload);
            }
            frame.push(1);
            frame.extend_from_slice(&inventory.revision.to_le_bytes());
            frame.push(inventory.equipped.map_or(0, |slot| slot + 1));
            for item in inventory.slots {
                frame.push(item.map_or(0, |item| match item.kind {
                    ToolKind::Shovel => 1,
                    ToolKind::Pickaxe => 2,
                }));
            }
        }
        ExcavationMessage::ActionResult { result } => {
            frame.push(2);
            match result {
                ActionResult::Committed {
                    action_id,
                    mutation_id,
                } => {
                    if *action_id == 0 || *mutation_id == 0 {
                        return Err(ProtocolError::InvalidPayload);
                    }
                    frame.push(1);
                    frame.extend_from_slice(&action_id.to_le_bytes());
                    frame.extend_from_slice(&mutation_id.to_le_bytes());
                }
                ActionResult::Rejected { action_id, reason } => {
                    if *action_id == 0 {
                        return Err(ProtocolError::InvalidPayload);
                    }
                    let tag = match reason {
                        ActionRejection::NotEquipped => 1,
                        ActionRejection::InvalidIntent => 2,
                        ActionRejection::OutOfReach => 3,
                        ActionRejection::Obstructed => 4,
                        ActionRejection::WrongMaterial => 5,
                        ActionRejection::Busy => 6,
                        ActionRejection::RateLimited => 7,
                        ActionRejection::Stale => 8,
                        ActionRejection::Capacity => 9,
                        ActionRejection::Storage => 10,
                        ActionRejection::UnsafeGeometry => 11,
                    };
                    frame.push(2);
                    frame.extend_from_slice(&action_id.to_le_bytes());
                    frame.push(tag);
                }
            }
        }
        ExcavationMessage::BatchBegin {
            sync_id,
            base_revision,
            revision,
            chunks,
            craters,
            planet_radius_m,
            full,
        } => {
            if *sync_id == 0
                || *chunks > 64
                || *craters as usize > minimium_world::terrain_edits::MAX_TERRAIN_EDITS
                || !planet_radius_m.is_finite()
                || !(1.0..=6_371_000.0).contains(planet_radius_m)
                || (*full && *base_revision != 0)
                || revision < base_revision
            {
                return Err(ProtocolError::InvalidPayload);
            }
            frame.push(3);
            frame.extend_from_slice(&sync_id.to_le_bytes());
            frame.extend_from_slice(&base_revision.to_le_bytes());
            frame.extend_from_slice(&revision.to_le_bytes());
            frame.extend_from_slice(&chunks.to_le_bytes());
            frame.extend_from_slice(&craters.to_le_bytes());
            frame.extend_from_slice(&planet_radius_m.to_le_bytes());
            frame.push(u8::from(*full));
        }
        ExcavationMessage::ChunkBegin {
            sync_id,
            coordinates,
            revision,
            total_bytes,
        } => {
            if *sync_id == 0
                || *revision == 0
                || *total_bytes as usize != MAX_EXCAVATION_CHUNK_BYTES
                || coordinates.iter().any(|c| !(-625_000..625_000).contains(c))
            {
                return Err(ProtocolError::InvalidPayload);
            }
            frame.push(4);
            frame.extend_from_slice(&sync_id.to_le_bytes());
            for coordinate in coordinates {
                frame.extend_from_slice(&coordinate.to_le_bytes());
            }
            frame.extend_from_slice(&revision.to_le_bytes());
            frame.extend_from_slice(&total_bytes.to_le_bytes());
        }
        ExcavationMessage::ChunkPart {
            sync_id,
            coordinates,
            revision,
            offset,
            bytes,
        } => {
            if *sync_id == 0
                || *revision == 0
                || bytes.is_empty()
                || bytes.len() > MAX_EXCAVATION_PART_BYTES
                || (*offset as usize)
                    .checked_add(bytes.len())
                    .is_none_or(|end| end > MAX_EXCAVATION_CHUNK_BYTES)
            {
                return Err(ProtocolError::InvalidPayload);
            }
            frame.push(5);
            frame.extend_from_slice(&sync_id.to_le_bytes());
            for coordinate in coordinates {
                frame.extend_from_slice(&coordinate.to_le_bytes());
            }
            frame.extend_from_slice(&revision.to_le_bytes());
            frame.extend_from_slice(&offset.to_le_bytes());
            frame.extend_from_slice(&(bytes.len() as u16).to_le_bytes());
            frame.extend_from_slice(bytes);
        }
        ExcavationMessage::ChunkEnd {
            sync_id,
            coordinates,
            revision,
        } => {
            if *sync_id == 0 || *revision == 0 {
                return Err(ProtocolError::InvalidPayload);
            }
            frame.push(6);
            frame.extend_from_slice(&sync_id.to_le_bytes());
            for coordinate in coordinates {
                frame.extend_from_slice(&coordinate.to_le_bytes());
            }
            frame.extend_from_slice(&revision.to_le_bytes());
        }
        ExcavationMessage::BatchEnd { sync_id, revision } => {
            if *sync_id == 0 {
                return Err(ProtocolError::InvalidPayload);
            }
            frame.push(7);
            frame.extend_from_slice(&sync_id.to_le_bytes());
            frame.extend_from_slice(&revision.to_le_bytes());
        }
        ExcavationMessage::FluidBegin {
            sync_id,
            revision,
            total_bytes,
            region_min,
            region_max,
        } => {
            if *sync_id == 0
                || *total_bytes == 0
                || *total_bytes as usize > MAX_FLUID_SNAPSHOT_BYTES
                || (0..3).any(|axis| {
                    region_min[axis] >= region_max[axis]
                        || region_max[axis].abs_diff(region_min[axis]) > 64
                        || !(-20_000_032..=20_000_032).contains(&region_min[axis])
                        || !(-20_000_032..=20_000_032).contains(&region_max[axis])
                })
            {
                return Err(ProtocolError::InvalidPayload);
            }
            frame.push(8);
            frame.extend_from_slice(&sync_id.to_le_bytes());
            frame.extend_from_slice(&revision.to_le_bytes());
            frame.extend_from_slice(&total_bytes.to_le_bytes());
            for coordinate in region_min {
                frame.extend_from_slice(&coordinate.to_le_bytes());
            }
            for coordinate in region_max {
                frame.extend_from_slice(&coordinate.to_le_bytes());
            }
        }
        ExcavationMessage::FluidPart {
            sync_id,
            revision,
            offset,
            bytes,
        } => {
            if *sync_id == 0
                || bytes.is_empty()
                || bytes.len() > MAX_EXCAVATION_PART_BYTES
                || (*offset as usize)
                    .checked_add(bytes.len())
                    .is_none_or(|end| end > MAX_FLUID_SNAPSHOT_BYTES)
            {
                return Err(ProtocolError::InvalidPayload);
            }
            frame.push(9);
            frame.extend_from_slice(&sync_id.to_le_bytes());
            frame.extend_from_slice(&revision.to_le_bytes());
            frame.extend_from_slice(&offset.to_le_bytes());
            frame.extend_from_slice(&(bytes.len() as u16).to_le_bytes());
            frame.extend_from_slice(bytes);
        }
        ExcavationMessage::FluidEnd { sync_id, revision } => {
            if *sync_id == 0 {
                return Err(ProtocolError::InvalidPayload);
            }
            frame.push(10);
            frame.extend_from_slice(&sync_id.to_le_bytes());
            frame.extend_from_slice(&revision.to_le_bytes());
        }
        ExcavationMessage::Crater {
            sync_id,
            planet_radius_m,
            stamp,
        } => {
            if *sync_id == 0
                || !planet_radius_m.is_finite()
                || !(1.0..=6_371_000.0).contains(planet_radius_m)
                || !stamp.valid()
            {
                return Err(ProtocolError::InvalidPayload);
            }
            frame.push(11);
            frame.extend_from_slice(&sync_id.to_le_bytes());
            frame.extend_from_slice(&planet_radius_m.to_le_bytes());
            frame.extend_from_slice(&stamp.id.to_le_bytes());
            for value in stamp.center {
                frame.extend_from_slice(&value.to_le_bytes());
            }
            frame.extend_from_slice(&stamp.radius_m.to_le_bytes());
            frame.extend_from_slice(&stamp.depth_m.to_le_bytes());
            frame.extend_from_slice(&stamp.seed.to_le_bytes());
        }
    }
    Ok(())
}
