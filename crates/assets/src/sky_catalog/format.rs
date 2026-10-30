use sha2::{Digest, Sha256};

use super::record::{StarCatalogue, StarRecord};
use super::{identity, CatalogError, InvalidRecordReason};

const HEADER_BYTES: usize = 96;
const RECORD_BYTES: usize = 64;
const RECORD_COUNT: usize = 500;

pub(super) fn decode(bytes: &[u8]) -> Result<StarCatalogue, CatalogError> {
    if bytes.len() != HEADER_BYTES + RECORD_COUNT * RECORD_BYTES {
        return Err(CatalogError::Length);
    }
    if bytes.get(0..8) != Some(b"MNSKY500") {
        return Err(CatalogError::Magic);
    }
    if read_u16(bytes, 8)? != 1
        || usize::from(read_u16(bytes, 10)?) != RECORD_BYTES
        || read_u32(bytes, 20)? != 0
        || read_f64(bytes, 24)? != 2000.0
    {
        return Err(CatalogError::UnsupportedSchema);
    }
    if read_u32(bytes, 12)? != identity::CATALOGUE_ID
        || bytes.get(32..64) != Some(&identity::SOURCE_CSV_SHA256)
    {
        return Err(CatalogError::IdentityMismatch);
    }
    if read_u32(bytes, 16)? != RECORD_COUNT as u32 {
        return Err(CatalogError::InvalidCount);
    }
    let payload_digest: [u8; 32] = Sha256::digest(&bytes[HEADER_BYTES..]).into();
    if bytes.get(64..96) != Some(&payload_digest) {
        return Err(CatalogError::DigestMismatch);
    }
    let asset_digest: [u8; 32] = Sha256::digest(bytes).into();
    if asset_digest != identity::ASSET_SHA256 {
        return Err(CatalogError::DigestMismatch);
    }

    let mut records = Vec::with_capacity(RECORD_COUNT);
    let mut seen_ids = std::collections::HashSet::with_capacity(RECORD_COUNT);
    let mut previous_magnitude = None;
    for index in 0..RECORD_COUNT {
        let offset = HEADER_BYTES + index * RECORD_BYTES;
        let record = decode_record(bytes, offset)?;
        if record.hyg_id == 0 {
            return invalid(index, InvalidRecordReason::ZeroId);
        }
        if !seen_ids.insert(record.hyg_id) {
            return invalid(index, InvalidRecordReason::DuplicateId);
        }
        if !record.apparent_v_mag.is_finite() {
            return invalid(index, InvalidRecordReason::NonFiniteMagnitude);
        }
        if previous_magnitude.is_some_and(|previous| record.apparent_v_mag < previous) {
            return invalid(index, InvalidRecordReason::MagnitudeOrder);
        }
        previous_magnitude = Some(record.apparent_v_mag);
        if !record.color_index_bv.is_finite() {
            return invalid(index, InvalidRecordReason::NonFiniteColor);
        }
        if !record.position_eqj_pc.iter().all(|value| value.is_finite()) {
            return invalid(index, InvalidRecordReason::NonFinitePosition);
        }
        let position_length = record.position_eqj_pc[0]
            .hypot(record.position_eqj_pc[1])
            .hypot(record.position_eqj_pc[2]);
        if position_length == 0.0 {
            return invalid(index, InvalidRecordReason::ZeroPosition);
        }
        if !record
            .velocity_eqj_pc_per_jyear
            .iter()
            .all(|value| value.is_finite())
        {
            return invalid(index, InvalidRecordReason::NonFiniteVelocity);
        }
        records.push(record);
    }
    Ok(StarCatalogue::new(records))
}

fn decode_record(bytes: &[u8], offset: usize) -> Result<StarRecord, CatalogError> {
    let record = bytes
        .get(offset..offset + RECORD_BYTES)
        .ok_or(CatalogError::Length)?;
    let mut position = [0.0; 3];
    let mut velocity = [0.0; 3];
    for axis in 0..3 {
        position[axis] = read_f64(record, 16 + axis * 8)?;
        velocity[axis] = read_f64(record, 40 + axis * 8)?;
    }
    Ok(StarRecord {
        hyg_id: read_u32(record, 0)?,
        hip_id: read_u32(record, 4)?,
        apparent_v_mag: read_f32(record, 8)?,
        color_index_bv: read_f32(record, 12)?,
        position_eqj_pc: position,
        velocity_eqj_pc_per_jyear: velocity,
    })
}

fn invalid<T>(index: usize, reason: InvalidRecordReason) -> Result<T, CatalogError> {
    Err(CatalogError::InvalidRecord {
        index: index as u32,
        reason,
    })
}

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16, CatalogError> {
    let data: [u8; 2] = bytes
        .get(offset..offset + 2)
        .ok_or(CatalogError::Length)?
        .try_into()
        .map_err(|_| CatalogError::Length)?;
    Ok(u16::from_le_bytes(data))
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, CatalogError> {
    let data: [u8; 4] = bytes
        .get(offset..offset + 4)
        .ok_or(CatalogError::Length)?
        .try_into()
        .map_err(|_| CatalogError::Length)?;
    Ok(u32::from_le_bytes(data))
}

fn read_f32(bytes: &[u8], offset: usize) -> Result<f32, CatalogError> {
    let data: [u8; 4] = bytes
        .get(offset..offset + 4)
        .ok_or(CatalogError::Length)?
        .try_into()
        .map_err(|_| CatalogError::Length)?;
    Ok(f32::from_le_bytes(data))
}

fn read_f64(bytes: &[u8], offset: usize) -> Result<f64, CatalogError> {
    let data: [u8; 8] = bytes
        .get(offset..offset + 8)
        .ok_or(CatalogError::Length)?
        .try_into()
        .map_err(|_| CatalogError::Length)?;
    Ok(f64::from_le_bytes(data))
}
