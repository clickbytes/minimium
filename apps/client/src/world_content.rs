use minimium_client_runtime::{
    ContentCancellation, ResolvedWorldContent, SessionFailure, WorldContentRequest,
    WorldContentResolver,
};
use minimium_world::world::{WorldInstanceId, WorldManifest, WorldMap};
use std::{collections::HashMap, path::PathBuf, sync::Arc};

pub(crate) struct SampleWorldContentResolver {
    known: HashMap<WorldInstanceId, (WorldManifest, Arc<WorldMap>)>,
    earth_package_dir: Option<PathBuf>,
}

impl SampleWorldContentResolver {
    pub(crate) fn new(
        maps: HashMap<WorldInstanceId, WorldMap>,
        manifests: HashMap<WorldInstanceId, WorldManifest>,
        earth_package_dir: Option<PathBuf>,
    ) -> Self {
        let known = maps
            .into_iter()
            .filter_map(|(instance, map)| {
                manifests
                    .get(&instance)
                    .copied()
                    .map(|manifest| (instance, (manifest, Arc::new(map))))
            })
            .collect();
        Self {
            known,
            earth_package_dir,
        }
    }

    fn resolve_known(
        &self,
        request: WorldContentRequest,
        cancellation: &ContentCancellation,
    ) -> Result<Option<ResolvedWorldContent>, SessionFailure> {
        if cancellation.is_cancelled() {
            return Err(failure(
                "client.world_content_cancelled",
                "world-content preparation was cancelled",
            ));
        }
        let Some((manifest, map)) = self.known.get(&request.ticket.instance) else {
            return Ok(None);
        };
        if *manifest != request.manifest {
            if manifest.source() == minimium_world::world::WorldSource::Authored {
                return Err(failure(
                    "client.world_content_known_mismatch",
                    "authored local map does not match the server world manifest",
                ));
            }
            return Ok(None);
        }
        if map.fingerprint() != request.expected_fingerprint {
            return Err(failure(
                "client.world_content_known_mismatch",
                "known local world map does not match the server fingerprint",
            ));
        }
        let geography = self.geography_for(*manifest, cancellation)?;
        Ok(Some(ResolvedWorldContent {
            ticket: request.ticket,
            manifest: *manifest,
            map: map.clone(),
            geography,
        }))
    }

    fn geography_for(
        &self,
        manifest: WorldManifest,
        cancellation: &ContentCancellation,
    ) -> Result<Option<Arc<dyn minimium_procgen::PreparedGeography>>, SessionFailure> {
        let Some(planet) = manifest.planet().filter(|planet| planet.mode == 1) else {
            return Ok(None);
        };
        let package = self.earth_package_dir.as_deref().ok_or_else(|| {
            failure(
                "client.earth_package_path_missing",
                "application did not configure the Earth package path",
            )
        })?;
        if cancellation.is_cancelled() {
            return Err(failure(
                "client.world_content_cancelled",
                "world-content preparation was cancelled",
            ));
        }
        let geography = minimium_asset_io::open_earth_package_cancellable(
            package,
            Some(planet.package_digest),
            || cancellation.is_cancelled(),
        )
        .map_err(SessionFailure::from_asset_error)?;
        let geography: Arc<dyn minimium_procgen::PreparedGeography> = geography;
        Ok(Some(geography))
    }

    fn resolve_sample(
        &self,
        request: WorldContentRequest,
        cancellation: &ContentCancellation,
    ) -> Result<ResolvedWorldContent, SessionFailure> {
        if request.manifest.source() != minimium_world::world::WorldSource::Generated {
            return Err(failure(
                "client.world_content_unsupported",
                "authored world content is unavailable from the generated-world provider",
            ));
        }
        let geography = self.geography_for(request.manifest, cancellation)?;
        let map =
            minimium_game_sample::materialize_manifest_map(request.manifest, geography.clone())
                .map_err(|error| {
                    SessionFailure::caused_by(error.code(), error.to_string(), error)
                })?;
        if cancellation.is_cancelled() {
            return Err(failure(
                "client.world_content_cancelled",
                "world-content preparation was cancelled",
            ));
        }
        if map.fingerprint() != request.expected_fingerprint {
            return Err(failure(
                "client.world_content_validation_failed",
                "generated content does not match the requested fingerprint",
            ));
        }
        Ok(ResolvedWorldContent {
            ticket: request.ticket,
            manifest: request.manifest,
            map: Arc::new(map),
            geography,
        })
    }
}

impl WorldContentResolver for SampleWorldContentResolver {
    fn resolve(
        &self,
        request: WorldContentRequest,
        cancellation: ContentCancellation,
    ) -> Result<ResolvedWorldContent, SessionFailure> {
        if let Some(content) = self.resolve_known(request, &cancellation)? {
            return Ok(content);
        }
        if cancellation.is_cancelled() {
            return Err(failure(
                "client.world_content_cancelled",
                "world-content preparation was cancelled",
            ));
        }
        self.resolve_sample(request, &cancellation)
    }
}

fn failure(code: &'static str, message: impl Into<String>) -> SessionFailure {
    SessionFailure::new(code, message)
}

pub(crate) fn authored_manifest(id: u128, map: &WorldMap) -> WorldManifest {
    use minimium_world::world::{
        GenerationRevision, GenerationSpec, GeneratorDigest, WorldContentRevision,
        WorldDefinitionId, WorldSeed, WorldTopology,
    };
    let generation = GenerationSpec::new(
        WorldSeed::new(0),
        GenerationRevision::new(5).expect("current generator revision is nonzero"),
        1,
    )
    .expect("authored position quantum is nonzero");
    WorldManifest::new(
        WorldDefinitionId::new(id).expect("authored definition ID is nonzero"),
        WorldTopology::Dungeon,
        generation,
        WorldContentRevision::new(1).expect("authored content revision is nonzero"),
        GeneratorDigest::authored_map_fingerprint(map.fingerprint()),
    )
}
