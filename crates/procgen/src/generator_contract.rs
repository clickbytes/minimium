use crate::GenerationError;
use minimium_world::world::{
    GenerationSpec, GeneratorDigest, WorldManifest, WorldSource, WorldTopology,
};

/// Checks the generator contract before a supplied manifest enters a runtime.
pub fn validate_world_manifest(manifest: WorldManifest) -> Result<(), GenerationError> {
    match (manifest.topology(), manifest.planet()) {
        (WorldTopology::Planet, Some(planet)) if planet.is_valid() => {}
        (WorldTopology::Planet, _) => {
            return Err(GenerationError::invalid_manifest(
                "generated planet manifest requires valid planet settings",
            ));
        }
        (WorldTopology::Dungeon, Some(_)) => {
            return Err(GenerationError::invalid_manifest(
                "generated dungeon manifest cannot contain planet settings",
            ));
        }
        (WorldTopology::Dungeon, None) => {}
    }
    if manifest.generation().revision().get() != 5 {
        return Err(GenerationError::unsupported_revision(
            manifest.generation().revision().get(),
        ));
    }
    match manifest.source() {
        WorldSource::Generated
            if manifest.generator_digest()
                == generator_digest(manifest.topology(), manifest.generation()) =>
        {
            Ok(())
        }
        WorldSource::Generated => Err(GenerationError::invalid_manifest(
            "digest does not match the world generator contract",
        )),
        // Authored digests identify the expected map fingerprint and are
        // verified against the map at the resolver/install boundary.
        WorldSource::Authored if manifest.planet().is_none() => Ok(()),
        WorldSource::Authored => Err(GenerationError::invalid_manifest(
            "authored manifests cannot contain generated planet settings",
        )),
    }
}

/// Digest of the code contract for one topology and generator revision.
pub fn generator_digest(topology: WorldTopology, generation: GenerationSpec) -> GeneratorDigest {
    let label = b"minimium-procgen/philox4x32-10/planet-sectors/v5".as_slice();
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
