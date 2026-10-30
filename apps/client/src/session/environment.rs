use super::{ClientSession, DisconnectReason};
use minimium_core::{
    environment::{AstronomyModelRevision, CatalogueId, EnvironmentDescriptor, EnvironmentProfile},
    time::{validate_sample_after, ClockSample},
    world::{WorldManifest, WorldTopology},
};
use minimium_protocol::WorldContext;

impl ClientSession {
    pub(super) fn install_initial_environment(
        &mut self,
        manifest: WorldManifest,
        sample: ClockSample,
        descriptor: EnvironmentDescriptor,
    ) -> Result<(), DisconnectReason> {
        validate_descriptor(manifest, sample, descriptor)?;
        self.validate_sample_progress(sample)?;
        self.clock_sample = Some(sample);
        self.environment_descriptor = Some(descriptor);
        self.pending_environment = None;
        self.clock.begin_session(sample);
        Ok(())
    }

    pub(super) fn prepare_environment(
        &mut self,
        transition_id: minimium_core::world::TransitionId,
        context: WorldContext,
        manifest: WorldManifest,
        sample: ClockSample,
        descriptor: EnvironmentDescriptor,
    ) -> Result<(), DisconnectReason> {
        validate_descriptor(manifest, sample, descriptor)?;
        self.validate_sample_progress(sample)?;
        self.pending_environment = Some((transition_id, context, sample, descriptor));
        self.clock_sample = Some(sample);
        self.clock.note_sample(sample);
        Ok(())
    }

    pub(super) fn commit_environment(
        &mut self,
        transition_id: minimium_core::world::TransitionId,
        context: WorldContext,
        manifest: WorldManifest,
        sample: ClockSample,
        descriptor: EnvironmentDescriptor,
    ) -> Result<(), DisconnectReason> {
        if !self
            .pending_environment
            .is_some_and(|(pending_id, pending_context, _, pending)| {
                pending_id == transition_id && pending_context == context && pending == descriptor
            })
        {
            return Err(DisconnectReason::EnvironmentMismatch);
        }
        validate_descriptor(manifest, sample, descriptor)?;
        self.validate_sample_progress(sample)?;
        self.clock_sample = Some(sample);
        self.environment_descriptor = Some(descriptor);
        self.pending_environment = None;
        self.clock.note_sample(sample);
        Ok(())
    }

    pub(super) fn apply_environment_sync(
        &mut self,
        context: WorldContext,
        sample: ClockSample,
        descriptor: EnvironmentDescriptor,
    ) -> Result<bool, DisconnectReason> {
        if self.world_context != Some(context) {
            return Ok(false);
        }
        if self.environment_descriptor != Some(descriptor) {
            return Err(DisconnectReason::EnvironmentMismatch);
        }
        validate_pair(sample, descriptor)?;
        self.validate_sample_progress(sample)?;
        self.clock_sample = Some(sample);
        self.clock.note_sample(sample);
        Ok(true)
    }

    pub(super) fn validate_probe_nonce(
        &self,
        nonce: minimium_core::time::SessionNonce,
    ) -> Result<(), DisconnectReason> {
        if self.clock.session_nonce_matches(nonce) {
            Ok(())
        } else {
            Err(DisconnectReason::EnvironmentMismatch)
        }
    }

    pub(super) fn cancel_pending_environment(
        &mut self,
        transition_id: minimium_core::world::TransitionId,
    ) {
        if self
            .pending_environment
            .is_some_and(|(pending_id, _, _, _)| pending_id == transition_id)
        {
            self.pending_environment = None;
        }
    }

    fn validate_sample_progress(&self, sample: ClockSample) -> Result<(), DisconnectReason> {
        let Some(previous) = self.clock_sample else {
            return Ok(());
        };
        if sample.session_nonce() != previous.session_nonce()
            || sample.clock_id() != previous.clock_id()
            || sample.revision() < previous.revision()
            || (sample.revision() == previous.revision()
                && (sample.mode() != previous.mode()
                    || sample.rate_ppm() != previous.rate_ppm()
                    || sample.offline_policy() != previous.offline_policy()
                    || sample.world_utc_ms() < previous.world_utc_ms()))
            || validate_sample_after(sample, previous).is_err()
        {
            return Err(DisconnectReason::EnvironmentMismatch);
        }
        Ok(())
    }
}

fn validate_descriptor(
    manifest: WorldManifest,
    sample: ClockSample,
    descriptor: EnvironmentDescriptor,
) -> Result<(), DisconnectReason> {
    validate_pair(sample, descriptor)?;
    let profile = match manifest.topology() {
        WorldTopology::Dungeon => EnvironmentProfile::Interior,
        WorldTopology::Planet if manifest.planet().is_some_and(|planet| planet.mode == 1) => {
            EnvironmentProfile::Earth
        }
        WorldTopology::Planet => EnvironmentProfile::EarthLike,
    };
    let identity = minimium_assets::embedded_sky_catalog_identity();
    if minimium_assets::embedded_sky_catalog().is_err()
        || descriptor.profile() != profile
        || descriptor.astronomy_model_revision() != AstronomyModelRevision::CURRENT
        || descriptor.catalogue_id() != CatalogueId::CURRENT
        || identity.catalogue_id != descriptor.catalogue_id().get()
        || identity.sha256 != *descriptor.catalogue_sha256().as_bytes()
    {
        return Err(DisconnectReason::EnvironmentMismatch);
    }
    Ok(())
}

fn validate_pair(
    sample: ClockSample,
    descriptor: EnvironmentDescriptor,
) -> Result<(), DisconnectReason> {
    if descriptor.clock_id() != sample.clock_id() {
        return Err(DisconnectReason::EnvironmentMismatch);
    }
    Ok(())
}
