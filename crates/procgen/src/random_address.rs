use crate::philox::philox4x32_10;
use minimium_core::world::{GenerationSpec, ObjectOriginId, SectorKey};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(u8)]
pub enum GenerationChannel {
    Terrain = 1,
    Objects = 2,
    DungeonLayout = 3,
}

/// A seekable random source whose address includes generation revision, channel
/// and logical sector. Calling samples in any order produces the same values.
#[derive(Clone, Copy, Debug)]
pub struct AddressedRandom {
    key: [u32; 2],
    base_counter: [u32; 4],
}

impl AddressedRandom {
    pub fn new(spec: GenerationSpec, channel: GenerationChannel, sector: SectorKey) -> Self {
        let seed = spec.seed().get();
        let key = [seed as u32, (seed >> 32) as u32];
        let mut address = StableAddressHash::new();
        address.feed(b"MINIMIUM_PROCGEN_PHILOX4X32_10_V1");
        address.feed(&((seed >> 64) as u64).to_le_bytes());
        address.feed(&spec.revision().get().to_le_bytes());
        address.feed(&spec.position_quantum_mm().to_le_bytes());
        address.feed(&[channel as u8]);
        encode_sector(&mut address, sector);
        Self {
            key,
            base_counter: address.finish(),
        }
    }

    pub fn sample_u32(self, draw_index: u64) -> u32 {
        let output = philox4x32_10(self.key, self.counter_for_block(draw_index / 4));
        output[(draw_index % 4) as usize]
    }

    /// Maps a local generated-object ordinal to a stable, nonzero origin ID.
    pub fn object_origin(self, local_ordinal: u64) -> ObjectOriginId {
        let output = philox4x32_10(self.key, self.counter_for_block(local_ordinal));
        let value = u128::from(output[0])
            | (u128::from(output[1]) << 32)
            | (u128::from(output[2]) << 64)
            | (u128::from(output[3]) << 96);
        ObjectOriginId::new(value)
            .unwrap_or_else(|| ObjectOriginId::new(1).expect("one is a nonzero object origin"))
    }

    fn counter_for_block(self, block: u64) -> [u32; 4] {
        let low = u64::from(self.base_counter[0]) | (u64::from(self.base_counter[1]) << 32);
        let high = u64::from(self.base_counter[2]) | (u64::from(self.base_counter[3]) << 32);
        let value = ((u128::from(high) << 64) | u128::from(low)).wrapping_add(u128::from(block));
        [
            value as u32,
            (value >> 32) as u32,
            (value >> 64) as u32,
            (value >> 96) as u32,
        ]
    }
}

fn encode_sector(hash: &mut StableAddressHash, sector: SectorKey) {
    match sector {
        SectorKey::Planet(key) => {
            hash.feed(&[0, key.face().index()]);
            let (u, v) = key.coordinates();
            hash.feed(&u.to_le_bytes());
            hash.feed(&v.to_le_bytes());
        }
        SectorKey::Dungeon(key) => {
            hash.feed(&[1]);
            let (x, y, z) = key.coordinates();
            hash.feed(&x.to_le_bytes());
            hash.feed(&y.to_le_bytes());
            hash.feed(&z.to_le_bytes());
        }
    }
}

struct StableAddressHash {
    first: u64,
    second: u64,
}

impl StableAddressHash {
    fn new() -> Self {
        Self {
            first: 0xCBF2_9CE4_8422_2325,
            second: 0x8422_2325_CBF2_9CE4,
        }
    }

    fn feed(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.first = (self.first ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01B3);
            self.second = (self.second ^ u64::from(*byte ^ 0xA5))
                .rotate_left(13)
                .wrapping_mul(0x9E37_79B1_85EB_CA87);
        }
    }

    fn finish(self) -> [u32; 4] {
        let first = avalanche(self.first);
        let second = avalanche(self.second ^ first.rotate_left(17));
        [
            first as u32,
            (first >> 32) as u32,
            second as u32,
            (second >> 32) as u32,
        ]
    }
}

fn avalanche(mut value: u64) -> u64 {
    value ^= value >> 30;
    value = value.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value ^= value >> 27;
    value = value.wrapping_mul(0x94D0_49BB_1331_11EB);
    value ^ (value >> 31)
}
