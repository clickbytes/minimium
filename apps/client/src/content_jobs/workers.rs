use minimium_asset_io::{material_http, ContentCache};
use minimium_client_runtime::{
    ContentCancellation, ContentJobResult, MaterialContentRequest, PreparedWorldContent,
    SessionFailure, WorldContentRequest, WorldContentResolver,
};
use std::{path::PathBuf, sync::Arc};

pub(super) fn world(
    request: WorldContentRequest,
    resolver: Arc<dyn WorldContentResolver>,
    cancellation: ContentCancellation,
) -> ContentJobResult {
    ContentJobResult::World(
        resolver
            .resolve(request, cancellation)
            .and_then(PreparedWorldContent::prepare)
            .map_err(|failure| failure.with_content_ticket(request.ticket)),
    )
}

pub(super) fn material(
    request: MaterialContentRequest,
    cache: PathBuf,
    cache_limit_bytes: u64,
    cancel: ContentCancellation,
) -> ContentJobResult {
    let result = (|| {
        let mut manifest = Vec::new();
        material_http::fetch_cancellable(
            request.endpoint,
            "manifest.ron",
            minimium_assets::terrain_material::MAX_MANIFEST_BYTES as u64,
            &mut manifest,
            || cancel.is_cancelled(),
        )?;
        let expected: String = request
            .ticket
            .manifest_hash
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        ContentCache::new(cache, cache_limit_bytes)?.install(
            &manifest,
            &expected,
            request.compressed,
            |name, size, out| {
                material_http::fetch_cancellable(request.endpoint, name, size, out, || {
                    cancel.is_cancelled()
                })
            },
        )
    })();
    ContentJobResult::Material(result.map_err(SessionFailure::from_asset_error))
}
