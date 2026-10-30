use super::world_context::WorldContext;
use super::{
    limits::{MAX_CHAT_BYTES, PLAYER_PROFILE_ID_BYTES},
    models::ProtocolError,
    validation::{valid_chat, valid_direction_component, valid_name},
};
use minimium_core::identity::PlayerProfileId;
use minimium_core::world::{
    GenerationRevision, GenerationSpec, GeneratorDigest, MembershipEpoch, Position3Mm,
    TransitionId, WorldContentRevision, WorldDefinitionId, WorldInstanceId, WorldManifest,
    WorldSeed, WorldTopology,
};
use std::sync::Arc;

pub(crate) struct Reader<'a> {
    payload: &'a [u8],
    offset: usize,
}

impl<'a> Reader<'a> {
    pub(crate) fn new(payload: &'a [u8]) -> Self {
        Self { payload, offset: 0 }
    }

    fn take(&mut self, count: usize) -> Result<&'a [u8], ProtocolError> {
        let end = self
            .offset
            .checked_add(count)
            .filter(|end| *end <= self.payload.len())
            .ok_or(ProtocolError::InvalidPayload)?;
        let bytes = &self.payload[self.offset..end];
        self.offset = end;
        Ok(bytes)
    }

    pub(crate) fn byte(&mut self) -> Result<u8, ProtocolError> {
        Ok(self.take(1)?[0])
    }

    pub(crate) fn boolean(&mut self) -> Result<bool, ProtocolError> {
        match self.byte()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(ProtocolError::InvalidPayload),
        }
    }

    pub(crate) fn u64(&mut self) -> Result<u64, ProtocolError> {
        let bytes: [u8; 8] = self
            .take(8)?
            .try_into()
            .map_err(|_| ProtocolError::InvalidPayload)?;
        Ok(u64::from_le_bytes(bytes))
    }

    pub(crate) fn u128(&mut self) -> Result<u128, ProtocolError> {
        let bytes: [u8; 16] = self
            .take(16)?
            .try_into()
            .map_err(|_| ProtocolError::InvalidPayload)?;
        Ok(u128::from_le_bytes(bytes))
    }

    pub(crate) fn u32(&mut self) -> Result<u32, ProtocolError> {
        let bytes: [u8; 4] = self
            .take(4)?
            .try_into()
            .map_err(|_| ProtocolError::InvalidPayload)?;
        Ok(u32::from_le_bytes(bytes))
    }

    pub(crate) fn fixed<const N: usize>(&mut self) -> Result<[u8; N], ProtocolError> {
        self.take(N)?
            .try_into()
            .map_err(|_| ProtocolError::InvalidPayload)
    }

    pub(crate) fn world_context(&mut self) -> Result<WorldContext, ProtocolError> {
        let instance = WorldInstanceId::new(self.u128()?).ok_or(ProtocolError::InvalidPayload)?;
        let content =
            WorldContentRevision::new(self.u64()?).ok_or(ProtocolError::InvalidPayload)?;
        let epoch = MembershipEpoch::new(self.u64()?).ok_or(ProtocolError::InvalidPayload)?;
        Ok(WorldContext::new(instance, content, epoch))
    }

    pub(crate) fn world_manifest(&mut self) -> Result<WorldManifest, ProtocolError> {
        let definition =
            WorldDefinitionId::new(self.u128()?).ok_or(ProtocolError::InvalidPayload)?;
        let topology = match self.byte()? {
            0 => WorldTopology::Planet,
            1 => WorldTopology::Dungeon,
            _ => return Err(ProtocolError::InvalidPayload),
        };
        let seed = WorldSeed::new(self.u128()?);
        let revision = GenerationRevision::new(self.u32()?).ok_or(ProtocolError::InvalidPayload)?;
        let quantum = self.u32()?;
        let generation =
            GenerationSpec::new(seed, revision, quantum).ok_or(ProtocolError::InvalidPayload)?;
        let content =
            WorldContentRevision::new(self.u64()?).ok_or(ProtocolError::InvalidPayload)?;
        let digest = GeneratorDigest::new(self.fixed::<32>()?);
        let mut manifest = WorldManifest::new(definition, topology, generation, content, digest);
        match self.byte()? {
            0 => {}
            1 => {
                let planet = minimium_core::world::PlanetDefinition {
                    mode: self.byte()?,
                    radius_mm: self.u64()?,
                    height_scale_permille: self.u32()?,
                    sea_level_mm: self.u64()? as i64,
                    mountain_limit_mm: self.u32()?,
                    detail_strength_mm: self.u32()?,
                    detail_wavelength_mm: self.u32()?,
                    mountain_band_frequency: self.u32()?,
                    package_digest: self.fixed::<32>()?,
                };
                if !planet.is_valid() {
                    return Err(ProtocolError::InvalidPayload);
                }
                manifest = manifest.with_planet(planet);
            }
            _ => return Err(ProtocolError::InvalidPayload),
        }
        Ok(manifest)
    }

    pub(crate) fn transition_id(&mut self) -> Result<TransitionId, ProtocolError> {
        TransitionId::new(self.u128()?).ok_or(ProtocolError::InvalidPayload)
    }

    pub(crate) fn u16(&mut self) -> Result<u16, ProtocolError> {
        let bytes: [u8; 2] = self
            .take(2)?
            .try_into()
            .map_err(|_| ProtocolError::InvalidPayload)?;
        Ok(u16::from_le_bytes(bytes))
    }

    pub(crate) fn vector(&mut self) -> Result<[f32; 3], ProtocolError> {
        let mut vector = [0.0; 3];
        for value in &mut vector {
            let bytes: [u8; 4] = self
                .take(4)?
                .try_into()
                .map_err(|_| ProtocolError::InvalidPayload)?;
            *value = f32::from_le_bytes(bytes);
        }
        if !vector.iter().all(|value| value.is_finite()) {
            return Err(ProtocolError::InvalidPayload);
        }
        Ok(vector)
    }

    pub(crate) fn position_mm(&mut self) -> Result<Position3Mm, ProtocolError> {
        let mut position = [0_i64; 3];
        for coordinate in &mut position {
            *coordinate = i64::from_le_bytes(
                self.take(8)?
                    .try_into()
                    .map_err(|_| ProtocolError::InvalidPayload)?,
            );
            if coordinate.unsigned_abs() > 7_000_000_000 {
                return Err(ProtocolError::InvalidPayload);
            }
        }
        Ok(Position3Mm(position))
    }

    pub(crate) fn direction(&mut self) -> Result<[f32; 2], ProtocolError> {
        let mut direction = [0.0; 2];
        for value in &mut direction {
            let bytes: [u8; 4] = self
                .take(4)?
                .try_into()
                .map_err(|_| ProtocolError::InvalidPayload)?;
            *value = f32::from_le_bytes(bytes);
            if !valid_direction_component(*value) {
                return Err(ProtocolError::InvalidPayload);
            }
        }
        Ok(direction)
    }

    pub(crate) fn profile_id(&mut self) -> Result<PlayerProfileId, ProtocolError> {
        self.take(PLAYER_PROFILE_ID_BYTES)?
            .try_into()
            .map(PlayerProfileId::from_bytes)
            .map_err(|_| ProtocolError::InvalidPayload)
    }

    pub(crate) fn name(&mut self) -> Result<String, ProtocolError> {
        Ok(self.name_text()?.to_owned())
    }

    pub(crate) fn shared_name(&mut self) -> Result<Arc<str>, ProtocolError> {
        Ok(Arc::from(self.name_text()?))
    }

    pub(crate) fn name_text(&mut self) -> Result<&'a str, ProtocolError> {
        let byte_count = self.byte()? as usize;
        let bytes = self.take(byte_count)?;
        let name = std::str::from_utf8(bytes).map_err(|_| ProtocolError::InvalidName)?;
        if !valid_name(name) {
            return Err(ProtocolError::InvalidName);
        }
        Ok(name)
    }

    pub(crate) fn chat_text(&mut self) -> Result<&'a str, ProtocolError> {
        let byte_count = self.byte()? as usize;
        if byte_count == 0 || byte_count > MAX_CHAT_BYTES {
            return Err(ProtocolError::InvalidPayload);
        }
        let bytes = self.take(byte_count)?;
        let text = std::str::from_utf8(bytes).map_err(|_| ProtocolError::InvalidPayload)?;
        if !valid_chat(text) {
            return Err(ProtocolError::InvalidPayload);
        }
        Ok(text)
    }

    pub(crate) fn finish(&self) -> Result<(), ProtocolError> {
        if self.offset == self.payload.len() {
            Ok(())
        } else {
            Err(ProtocolError::InvalidPayload)
        }
    }
}
