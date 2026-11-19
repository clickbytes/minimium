use super::super::{
    limits::*,
    models::{ExcavationMessage, ProtocolError, ServerMessage},
    reading::Reader,
};

pub(super) fn decode(reader: &mut Reader<'_>) -> Result<ServerMessage, ProtocolError> {
    use minimium_world::items::{ActionRejection, ActionResult, Inventory, ItemRecord, ToolKind};

    let context = reader.world_context()?;
    let message = match reader.byte()? {
        1 => {
            let revision = reader.u64()?;
            let equipped = match reader.byte()? {
                0 => None,
                value @ 1..=2 => Some(value - 1),
                _ => return Err(ProtocolError::InvalidPayload),
            };
            let slots = [reader.byte()?, reader.byte()?].map(|kind| match kind {
                0 => Ok(None),
                1 => Ok(Some(ItemRecord {
                    kind: ToolKind::Shovel,
                })),
                2 => Ok(Some(ItemRecord {
                    kind: ToolKind::Pickaxe,
                })),
                _ => Err(ProtocolError::InvalidPayload),
            });
            let inventory = Inventory {
                slots: [slots[0].clone()?, slots[1].clone()?],
                equipped,
                revision,
            };
            if equipped.is_some_and(|slot| inventory.slots[usize::from(slot)].is_none()) {
                return Err(ProtocolError::InvalidPayload);
            }
            ExcavationMessage::Inventory { inventory }
        }
        2 => {
            let tag = reader.byte()?;
            let action_id = reader.u64()?;
            if action_id == 0 {
                return Err(ProtocolError::InvalidPayload);
            }
            let result = match tag {
                1 => {
                    let mutation_id = reader.u64()?;
                    if mutation_id == 0 {
                        return Err(ProtocolError::InvalidPayload);
                    }
                    ActionResult::Committed {
                        action_id,
                        mutation_id,
                    }
                }
                2 => {
                    let reason = match reader.byte()? {
                        1 => ActionRejection::NotEquipped,
                        2 => ActionRejection::InvalidIntent,
                        3 => ActionRejection::OutOfReach,
                        4 => ActionRejection::Obstructed,
                        5 => ActionRejection::WrongMaterial,
                        12 => ActionRejection::Paused,
                        6 => ActionRejection::Busy,
                        7 => ActionRejection::RateLimited,
                        8 => ActionRejection::Stale,
                        9 => ActionRejection::Capacity,
                        10 => ActionRejection::Storage,
                        11 => ActionRejection::UnsafeGeometry,
                        _ => return Err(ProtocolError::InvalidPayload),
                    };
                    ActionResult::Rejected { action_id, reason }
                }
                _ => return Err(ProtocolError::InvalidPayload),
            };
            ExcavationMessage::ActionResult { result }
        }
        3 => {
            let sync_id = reader.u64()?;
            let base_revision = reader.u64()?;
            let revision = reader.u64()?;
            let chunks = reader.u16()?;
            let craters = reader.u16()?;
            let planet_radius_m = f64::from_le_bytes(reader.fixed()?);
            let full = reader.boolean()?;
            if sync_id == 0
                || chunks > 64
                || craters as usize > minimium_world::terrain_edits::MAX_TERRAIN_EDITS
                || !planet_radius_m.is_finite()
                || !(1.0..=6_371_000.0).contains(&planet_radius_m)
                || (full && base_revision != 0)
                || revision < base_revision
            {
                return Err(ProtocolError::InvalidPayload);
            }
            ExcavationMessage::BatchBegin {
                sync_id,
                base_revision,
                revision,
                chunks,
                craters,
                planet_radius_m,
                full,
            }
        }
        4 => {
            let sync_id = reader.u64()?;
            let coordinates = [reader.i32()?, reader.i32()?, reader.i32()?];
            let revision = reader.u64()?;
            let total_bytes = reader.u32()?;
            if sync_id == 0
                || revision == 0
                || total_bytes as usize != MAX_EXCAVATION_CHUNK_BYTES
                || coordinates.iter().any(|c| !(-625_000..625_000).contains(c))
            {
                return Err(ProtocolError::InvalidPayload);
            }
            ExcavationMessage::ChunkBegin {
                sync_id,
                coordinates,
                revision,
                total_bytes,
            }
        }
        5 => {
            let sync_id = reader.u64()?;
            let coordinates = [reader.i32()?, reader.i32()?, reader.i32()?];
            let revision = reader.u64()?;
            let offset = reader.u32()?;
            let bytes = reader.bounded_bytes(MAX_EXCAVATION_PART_BYTES)?;
            if sync_id == 0
                || revision == 0
                || bytes.is_empty()
                || (offset as usize)
                    .checked_add(bytes.len())
                    .is_none_or(|end| end > MAX_EXCAVATION_CHUNK_BYTES)
            {
                return Err(ProtocolError::InvalidPayload);
            }
            ExcavationMessage::ChunkPart {
                sync_id,
                coordinates,
                revision,
                offset,
                bytes,
            }
        }
        6 => {
            let sync_id = reader.u64()?;
            let coordinates = [reader.i32()?, reader.i32()?, reader.i32()?];
            let revision = reader.u64()?;
            if sync_id == 0 || revision == 0 {
                return Err(ProtocolError::InvalidPayload);
            }
            ExcavationMessage::ChunkEnd {
                sync_id,
                coordinates,
                revision,
            }
        }
        7 => {
            let sync_id = reader.u64()?;
            let revision = reader.u64()?;
            if sync_id == 0 {
                return Err(ProtocolError::InvalidPayload);
            }
            ExcavationMessage::BatchEnd { sync_id, revision }
        }
        8 => {
            let sync_id = reader.u64()?;
            let revision = reader.u64()?;
            let total_bytes = reader.u32()?;
            let region_min = [reader.i32()?, reader.i32()?, reader.i32()?];
            let region_max = [reader.i32()?, reader.i32()?, reader.i32()?];
            if sync_id == 0 || total_bytes == 0 || total_bytes as usize > MAX_FLUID_SNAPSHOT_BYTES {
                return Err(ProtocolError::InvalidPayload);
            }
            if (0..3).any(|axis| {
                region_min[axis] >= region_max[axis]
                    || region_max[axis].abs_diff(region_min[axis]) > 64
                    || !(-20_000_032..=20_000_032).contains(&region_min[axis])
                    || !(-20_000_032..=20_000_032).contains(&region_max[axis])
            }) {
                return Err(ProtocolError::InvalidPayload);
            }
            ExcavationMessage::FluidBegin {
                sync_id,
                revision,
                total_bytes,
                region_min,
                region_max,
            }
        }
        9 => {
            let sync_id = reader.u64()?;
            let revision = reader.u64()?;
            let offset = reader.u32()?;
            let bytes = reader.bounded_bytes(MAX_EXCAVATION_PART_BYTES)?;
            if sync_id == 0
                || bytes.is_empty()
                || (offset as usize)
                    .checked_add(bytes.len())
                    .is_none_or(|end| end > MAX_FLUID_SNAPSHOT_BYTES)
            {
                return Err(ProtocolError::InvalidPayload);
            }
            ExcavationMessage::FluidPart {
                sync_id,
                revision,
                offset,
                bytes,
            }
        }
        10 => {
            let sync_id = reader.u64()?;
            let revision = reader.u64()?;
            if sync_id == 0 {
                return Err(ProtocolError::InvalidPayload);
            }
            ExcavationMessage::FluidEnd { sync_id, revision }
        }
        11 => {
            let sync_id = reader.u64()?;
            let planet_radius_m = f64::from_le_bytes(reader.fixed()?);
            let stamp = minimium_world::terrain_edits::CraterStamp {
                id: reader.u64()?,
                center: [
                    f64::from_le_bytes(reader.fixed()?),
                    f64::from_le_bytes(reader.fixed()?),
                    f64::from_le_bytes(reader.fixed()?),
                ],
                radius_m: f64::from_le_bytes(reader.fixed()?),
                depth_m: f64::from_le_bytes(reader.fixed()?),
                seed: reader.u64()?,
            };
            if sync_id == 0
                || !planet_radius_m.is_finite()
                || !(1.0..=6_371_000.0).contains(&planet_radius_m)
                || !stamp.valid()
            {
                return Err(ProtocolError::InvalidPayload);
            }
            ExcavationMessage::Crater {
                sync_id,
                planet_radius_m,
                stamp,
            }
        }
        _ => return Err(ProtocolError::InvalidPayload),
    };
    Ok(ServerMessage::Excavation { context, message })
}
