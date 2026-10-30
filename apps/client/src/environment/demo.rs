use super::{
    diagnostics::{ClockDiagnostics, ClockSource},
    EnvironmentFrameInput, SynchronizationQuality,
};
use minimium_core::{
    environment::{
        AstronomyModelRevision, CatalogueId, CatalogueSha256, EnvironmentDescriptor,
        EnvironmentProfile,
    },
    time::{
        evaluate_clock, ClockAnchor, ClockMode, MonotonicMs, OfflinePolicy, TimeScalePpm,
        WorldTimeMs,
    },
    world::{WorldManifest, WorldTopology},
};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

/// Explicit offline-demo adapter. Network sessions never fall back to this clock.
pub(crate) struct LocalDemoEnvironment {
    anchor: ClockAnchor,
    started_at: Instant,
}

impl LocalDemoEnvironment {
    pub(crate) fn new() -> Option<Self> {
        let utc_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .ok()?
            .as_millis();
        let utc_ms = i64::try_from(utc_ms).ok()?;
        let world_time = WorldTimeMs::new(utc_ms).ok()?;
        let anchor = ClockAnchor::initial(
            ClockMode::RealTime,
            world_time,
            MonotonicMs::new(0),
            TimeScalePpm::ONE_X,
            OfflinePolicy::Pause,
        )
        .ok()?;
        Some(Self {
            anchor,
            started_at: Instant::now(),
        })
    }

    pub(crate) fn frame(
        &self,
        manifest: WorldManifest,
        context: Option<minimium_protocol::WorldContext>,
    ) -> EnvironmentFrameInput {
        let elapsed_ms = self.started_at.elapsed().as_millis();
        let monotonic_ms = match u64::try_from(elapsed_ms) {
            Ok(value) => MonotonicMs::new(value),
            Err(_) => return EnvironmentFrameInput::unavailable(),
        };
        let world_time = match evaluate_clock(self.anchor, monotonic_ms) {
            Ok(value) => value.get(),
            Err(_) => return EnvironmentFrameInput::unavailable(),
        };
        let profile = match manifest.topology() {
            WorldTopology::Dungeon => EnvironmentProfile::Interior,
            WorldTopology::Planet if manifest.planet().is_some_and(|planet| planet.mode == 1) => {
                EnvironmentProfile::Earth
            }
            WorldTopology::Planet => EnvironmentProfile::EarthLike,
        };
        let identity = minimium_assets::embedded_sky_catalog_identity();
        if minimium_assets::embedded_sky_catalog().is_err()
            || identity.catalogue_id != CatalogueId::CURRENT.get()
        {
            return EnvironmentFrameInput::unavailable();
        }
        let descriptor = EnvironmentDescriptor::new(
            self.anchor.clock_id(),
            profile,
            AstronomyModelRevision::CURRENT,
            CatalogueId::CURRENT,
            CatalogueSha256::new(identity.sha256),
        );
        EnvironmentFrameInput {
            estimated_world_utc_ms: Some(world_time),
            render_world_utc_ms: Some(world_time),
            descriptor: Some(descriptor),
            context,
            revision: Some(self.anchor.revision().get()),
            rate_ppm: Some(self.anchor.rate_ppm().get()),
            quality: SynchronizationQuality::LocalDemo,
            clock_diagnostics: Some(ClockDiagnostics {
                source: ClockSource::Demo,
                clock_id: Some(self.anchor.clock_id()),
                mode: Some(self.anchor.mode()),
                offline_policy: Some(self.anchor.offline_policy()),
                sample_age_real_ms: None,
                clock_offset_ms: None,
                probe_rtt_ms: None,
                samples_accepted: Some(0),
                samples_rejected: Some(0),
            }),
        }
    }
}
