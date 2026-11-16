use minimium_procgen::{
    generator_digest, GenerationErrorKind, PlanetGenerationSource, PlanetReliefSamplingPolicy,
    PlanetTerrain, PreparedTerrainSource, ResolvedPlanetReliefPolicy, TerrainInputs,
    TerrainMaterialProfile, TerrainOverlaySnapshot,
};
use minimium_world::world::{
    GenerationRevision, GenerationSpec, GeneratorDigest, MapBox, PeakRelief, PlanetDefinition,
    WorldContentRevision, WorldDefinitionId, WorldInstanceId, WorldManifest, WorldMap, WorldSeed,
    WorldTopology,
};

fn demo_planet(mode: u8) -> PlanetDefinition {
    PlanetDefinition {
        mode,
        radius_mm: 6_371_000_000,
        height_scale_permille: 1000,
        sea_level_mm: 0,
        mountain_limit_mm: 5_000_000,
        detail_strength_mm: 1000,
        detail_wavelength_mm: 100_000,
        mountain_band_frequency: 16,
        package_digest: [0x5a; 32],
        peak_relief: PeakRelief::NONE,
    }
}

fn authored_map() -> WorldMap {
    WorldMap::from_parts(
        [0.0, 3.0, 0.0],
        vec![MapBox {
            center: [0.0, 0.0, 0.0],
            size: [8.0, 1.0, 8.0],
            color: [0.3, 0.3, 0.3],
            solid: true,
        }],
        Vec::new(),
    )
    .expect("valid example map")
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let generation = GenerationSpec::new(
        WorldSeed::new(7),
        GenerationRevision::new(6).expect("nonzero generator revision"),
        1,
    )
    .expect("nonzero position quantum");

    // The explicit recipe is used while generating a procedural map, before
    // its final map fingerprint is available.
    let procedural = demo_planet(0);
    let recipe = PlanetGenerationSource::new(Some(procedural), None)?;
    let surface = minimium_procgen::PlanetSurfaceSpec::new(
        procedural.radius_mm,
        u64::from(procedural.mountain_limit_mm),
    )
    .expect("valid example surface");
    let generated_manifest = WorldManifest::new(
        WorldDefinitionId::new(7).expect("nonzero definition ID"),
        WorldTopology::Planet,
        generation,
        WorldContentRevision::new(1).expect("nonzero content revision"),
        generator_digest(WorldTopology::Planet, generation),
    )
    .with_planet(procedural);
    let terrain = PlanetTerrain::new_with_relief_policy(
        generation,
        surface,
        Some(procedural),
        recipe,
        PlanetReliefSamplingPolicy::identity(),
    )?;
    let generated_map = WorldMap::from_parts(
        [6_400_000.0, 0.0, 0.0],
        Vec::new(),
        minimium_procgen::materialize_planet_overview(terrain)?,
    )?;
    let generated_instance = WorldInstanceId::new(7).expect("nonzero instance ID");
    let generated_profile = TerrainMaterialProfile::new(1500)
        .expect("bounded material profile")
        .bind(generated_manifest, generated_map.fingerprint());
    let generated_relief = ResolvedPlanetReliefPolicy::bind(
        PlanetReliefSamplingPolicy::identity(),
        generated_manifest,
        generated_map.fingerprint(),
    );
    let source = PreparedTerrainSource::new(
        generated_instance,
        generated_manifest,
        &generated_map,
        None,
        generated_profile,
        Some(generated_relief),
    )?;
    let _inputs = TerrainInputs::new(
        source,
        TerrainOverlaySnapshot::empty_for(generated_instance),
    )?;

    let earth = demo_planet(1);
    let earth_manifest = WorldManifest::new(
        WorldDefinitionId::new(8).expect("nonzero definition ID"),
        WorldTopology::Planet,
        generation,
        WorldContentRevision::new(1).expect("nonzero content revision"),
        generator_digest(WorldTopology::Planet, generation),
    )
    .with_planet(earth);
    let earth_profile = TerrainMaterialProfile::new(1500)
        .expect("bounded material profile")
        .bind(earth_manifest, generated_map.fingerprint());
    let earth_relief = ResolvedPlanetReliefPolicy::bind(
        PlanetReliefSamplingPolicy::identity(),
        earth_manifest,
        generated_map.fingerprint(),
    );
    let earth_instance = WorldInstanceId::new(8).expect("nonzero instance ID");
    let missing_earth = PreparedTerrainSource::new(
        earth_instance,
        earth_manifest,
        &generated_map,
        None,
        earth_profile,
        Some(earth_relief),
    )
    .unwrap_err();
    assert_eq!(
        missing_earth.kind(),
        GenerationErrorKind::Sampling(minimium_procgen::EarthSampleError::PackageUnavailable),
    );

    // Authored dungeon content needs neither planet settings nor geography.
    let map = authored_map();
    let instance = WorldInstanceId::new(2).expect("nonzero instance ID");
    let manifest = WorldManifest::new(
        WorldDefinitionId::new(2).expect("nonzero definition ID"),
        WorldTopology::Dungeon,
        generation,
        WorldContentRevision::new(1).expect("nonzero content revision"),
        GeneratorDigest::authored_map_fingerprint(map.fingerprint()),
    );
    let profile = TerrainMaterialProfile::new(1500)
        .expect("bounded material profile")
        .bind(manifest, map.fingerprint());
    let source = PreparedTerrainSource::new(instance, manifest, &map, None, profile, None)?;
    let inputs = TerrainInputs::new(source, TerrainOverlaySnapshot::empty_for(instance))?;
    let _ = inputs;
    Ok(())
}
