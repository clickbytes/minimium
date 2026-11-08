//! Content preparation has its own five-minute deadline, independent of Welcome.
use super::*;
use minimium_asset_io::{material_http, materials::MaterialPackage, ContentCache};
use minimium_environment::climate::ClimateScenario;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;

#[derive(Default)]
pub(super) struct ContentPreparation {
    pub capability: Option<bool>,
    pub started: Option<Instant>,
    pub hash: [u8; 32],
    pub climate: Arc<ClimateScenario>,
    pub awaiting_gpu: bool,
    pub package: Option<MaterialPackage>,
    pub(super) worker: Option<JoinHandle<Result<MaterialPackage, minimium_asset_io::AssetIoError>>>,
    pub(super) cancel: Arc<AtomicBool>,
}

impl Drop for ContentPreparation {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

impl ClientSession {
    pub fn climate_scenario(&self) -> Arc<ClimateScenario> {
        self.content.climate.clone()
    }
    pub fn content_awaiting_gpu(&self) -> bool {
        self.content.awaiting_gpu
    }
    pub fn take_material_package(&mut self) -> Option<MaterialPackage> {
        self.content.package.take()
    }

    /// The presentation host acknowledges only after successful GPU installation.
    pub fn finish_content(&mut self) -> Result<(), ProtocolError> {
        if !self.content.awaiting_gpu {
            return Err(ProtocolError::InvalidPayload);
        }
        self.append_encoded(&ClientMessage::ContentReady(self.content.hash))?;
        self.content.awaiting_gpu = false;
        self.content.started = None;
        self.state_since = Instant::now();
        Ok(())
    }

    pub(super) fn begin_content(
        &mut self,
        hash: [u8; 32],
        port: u16,
        climate: ClimateScenario,
    ) -> Result<(), SessionFailure> {
        if self.state != SessionState::AwaitingWelcome
            || self.content.started.is_some()
            || self.content.capability.is_none()
        {
            return Err(SessionFailure::new(
                "session.unexpected_content_offer",
                "unexpected content offer",
            ));
        }
        climate
            .validate()
            .map_err(|error| SessionFailure::new("session.invalid_climate_scenario", error))?;
        self.content.climate = Arc::new(climate);
        self.content.started = Some(Instant::now());
        self.content.hash = hash;
        if hash == [0; 32] {
            self.content.awaiting_gpu = true;
            return Ok(());
        }
        if port == 0 {
            return Err(SessionFailure::new(
                "session.material_endpoint_missing",
                "required material package has no HTTP endpoint",
            ));
        }
        let mut endpoint = self
            .stream
            .as_ref()
            .ok_or_else(|| {
                SessionFailure::new("session.game_socket_missing", "missing game socket")
            })?
            .peer_addr()
            .map_err(|error| {
                SessionFailure::caused_by(
                    "session.peer_address_failed",
                    "could not read connected game peer address",
                    error,
                )
            })?;
        endpoint.set_port(port);
        let compressed = self.content.capability.unwrap();
        let cache = self.content_cache_dir.clone().ok_or_else(|| {
            SessionFailure::new(
                "session.material_cache_path_missing",
                "application did not supply the material cache path",
            )
        })?;
        let expected: String = hash.iter().map(|b| format!("{b:02x}")).collect();
        let cancel = self.content.cancel.clone();
        let cache_limit_bytes = self.content_cache_limit_bytes;
        self.content.worker = Some(
            std::thread::Builder::new()
                .name("material-download".into())
                .spawn(move || {
                    let mut manifest = Vec::new();
                    material_http::fetch_cancellable(
                        endpoint,
                        "manifest.ron",
                        minimium_assets::terrain_material::MAX_MANIFEST_BYTES as u64,
                        &mut manifest,
                        || cancel.load(Ordering::Relaxed),
                    )?;
                    ContentCache::new(cache, cache_limit_bytes)?.install(
                        &manifest,
                        &expected,
                        compressed,
                        |name, size, out| {
                            material_http::fetch_cancellable(endpoint, name, size, out, || {
                                cancel.load(Ordering::Relaxed)
                            })
                        },
                    )
                })
                .map_err(|error| {
                    SessionFailure::from_asset_error(minimium_asset_io::AssetIoError::io(
                        "start material download worker",
                        error,
                    ))
                })?,
        );
        Ok(())
    }

    pub(super) fn poll_content(&mut self) -> Result<(), SessionFailure> {
        if self
            .content
            .started
            .is_some_and(|start| start.elapsed() >= Duration::from_secs(300))
        {
            return Err(SessionFailure::new(
                "session.content_preparation_timeout",
                "material preparation deadline exceeded; reconnect to retry",
            ));
        }
        if self
            .content
            .worker
            .as_ref()
            .is_some_and(|worker| worker.is_finished())
        {
            let package = self
                .content
                .worker
                .take()
                .unwrap()
                .join()
                .map_err(|_| {
                    SessionFailure::new(
                        "session.material_worker_failed",
                        "material worker thread terminated unexpectedly",
                    )
                })?
                .map_err(SessionFailure::from_asset_error)?;
            self.content.package = Some(package);
            self.content.awaiting_gpu = true;
        }
        Ok(())
    }
}
