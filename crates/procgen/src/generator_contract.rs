use minimium_world::world::{GenerationSpec, GeneratorDigest, WorldManifest, WorldTopology};

/// Checks the generator contract before a supplied manifest enters a runtime.
pub fn validate_world_manifest(manifest: WorldManifest) -> Result<(), String> {
    if manifest.planet().is_some_and(|p| {
        !p.is_valid() || (p.peak_relief.enabled() && manifest.generation().revision().get() != 5)
    }) {
        return Err("peak relief requires a valid Earth generator v5 definition".into());
    }
    if manifest.generator_digest() == generator_digest(manifest.topology(), manifest.generation()) {
        Ok(())
    } else {
        Err("digest does not match the world generator contract".into())
    }
}

/// Digest of the code contract for one topology and generator revision.
pub fn generator_digest(topology: WorldTopology, generation: GenerationSpec) -> GeneratorDigest {
    let revision = generation.revision().get();
    let label = if revision == 1 {
        b"minimium-procgen/philox4x32-10/v1".as_slice()
    } else if revision >= 5 {
        b"minimium-procgen/philox4x32-10/planet-sectors/v5".as_slice()
    } else if revision >= 4 {
        b"minimium-procgen/philox4x32-10/planet-sectors/v4".as_slice()
    } else if revision >= 3 {
        b"minimium-procgen/philox4x32-10/planet-sectors/v3".as_slice()
    } else {
        b"minimium-procgen/philox4x32-10/planet-sectors/v2".as_slice()
    };
    digest_with_label(label, topology, generation)
}

fn digest_with_label(
    label: &[u8],
    topology: WorldTopology,
    generation: GenerationSpec,
) -> GeneratorDigest {
    let mut hash = 0xcbf29ce484222325_u64;
    let mut feed = |bytes: &[u8]| {
        for byte in bytes {
            hash = (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3);
        }
    };
    feed(label);
    feed(&[match topology {
        WorldTopology::Planet => 0,
        WorldTopology::Dungeon => 1,
    }]);
    feed(&generation.revision().get().to_le_bytes());
    feed(&generation.position_quantum_mm().to_le_bytes());
    let mut bytes = [0_u8; 32];
    for (index, chunk) in bytes.as_chunks_mut::<8>().0.iter_mut().enumerate() {
        let value = hash.rotate_left((index * 11) as u32) ^ index as u64;
        chunk.copy_from_slice(&value.to_le_bytes());
    }
    GeneratorDigest::new(bytes)
}
