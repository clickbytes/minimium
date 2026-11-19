//! Opaque, fixed-width cursor token. The v3 form fits the 512-byte admin
//! request even with maximal world/entity IDs and full correlation fields.

use super::{
    CommandViewCursor, CommandViewCursorIssuer, CommandViewError, CommandViewKind, CommandViewScope,
};
use minimium_world::{entity::EntityId, world::WorldInstanceId};
use std::sync::Arc;

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
const RAW_LEN: usize = 98;
const PREFIX: &str = "v3.";

pub(super) fn encode(cursor: &CommandViewCursor) -> String {
    let mut raw = [0u8; RAW_LEN];
    raw[..8].copy_from_slice(&(Arc::as_ptr(&cursor.identity) as usize as u64).to_be_bytes());
    raw[8] = cursor.query.code();
    let (tag, world, entity) = match cursor.scope {
        CommandViewScope::None => (0, 0, 0),
        CommandViewScope::World(world) => (1, world.get(), 0),
        CommandViewScope::Entity { world, entity } => (2, world.get(), entity.get()),
    };
    raw[9] = tag;
    raw[10..26].copy_from_slice(&world.to_be_bytes());
    raw[26..34].copy_from_slice(&entity.to_be_bytes());
    raw[34..50].copy_from_slice(&cursor.revision.to_be_bytes());
    raw[50..66].copy_from_slice(&cursor.secondary_revision.to_be_bytes());
    raw[66..82].copy_from_slice(&cursor.tertiary_revision.to_be_bytes());
    raw[82..98].copy_from_slice(&cursor.after.to_be_bytes());
    let mut out = String::with_capacity(PREFIX.len() + 131);
    out.push_str(PREFIX);
    for chunk in raw.chunks(3) {
        let bits = (u32::from(chunk[0]) << 16)
            | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8)
            | u32::from(*chunk.get(2).unwrap_or(&0));
        out.push(ALPHABET[((bits >> 18) & 63) as usize] as char);
        out.push(ALPHABET[((bits >> 12) & 63) as usize] as char);
        if chunk.len() > 1 {
            out.push(ALPHABET[((bits >> 6) & 63) as usize] as char);
        }
        if chunk.len() > 2 {
            out.push(ALPHABET[(bits & 63) as usize] as char);
        }
    }
    out
}

pub(super) fn decode(
    issuer: &CommandViewCursorIssuer,
    token: &str,
) -> Result<CommandViewCursor, CommandViewError> {
    let body = token
        .strip_prefix(PREFIX)
        .ok_or(CommandViewError::StaleCursor)?;
    if body.len() != 131 {
        return Err(CommandViewError::StaleCursor);
    }
    let mut raw = [0u8; RAW_LEN];
    let mut index = 0;
    let mut acc = 0u32;
    let mut bits = 0;
    for byte in body.bytes() {
        let digit = ALPHABET
            .iter()
            .position(|candidate| *candidate == byte)
            .ok_or(CommandViewError::StaleCursor)? as u32;
        acc = (acc << 6) | digit;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            let slot = raw.get_mut(index).ok_or(CommandViewError::StaleCursor)?;
            *slot = (acc >> bits) as u8;
            index += 1;
            acc &= (1 << bits) - 1;
        }
    }
    if index != RAW_LEN || acc != 0 {
        return Err(CommandViewError::StaleCursor);
    }
    let get_u128 = |range: std::ops::Range<usize>| {
        u128::from_be_bytes(raw[range].try_into().expect("fixed cursor width"))
    };
    let identity = u64::from_be_bytes(raw[..8].try_into().expect("fixed cursor width"));
    if identity != Arc::as_ptr(&issuer.0) as usize as u64 {
        return Err(CommandViewError::StaleCursor);
    }
    let query = CommandViewKind::from_code(raw[8]).ok_or(CommandViewError::StaleCursor)?;
    let world = get_u128(10..26);
    let entity = u64::from_be_bytes(raw[26..34].try_into().expect("fixed cursor width"));
    let scope = match raw[9] {
        0 if world == 0 && entity == 0 => CommandViewScope::None,
        1 if entity == 0 => CommandViewScope::World(
            WorldInstanceId::new(world).ok_or(CommandViewError::StaleCursor)?,
        ),
        2 => CommandViewScope::Entity {
            world: WorldInstanceId::new(world).ok_or(CommandViewError::StaleCursor)?,
            entity: EntityId::new(entity).ok_or(CommandViewError::StaleCursor)?,
        },
        _ => return Err(CommandViewError::StaleCursor),
    };
    Ok(issuer.issue_versions(
        query,
        scope,
        get_u128(34..50),
        get_u128(50..66),
        get_u128(66..82),
        get_u128(82..98),
    ))
}
