use super::*;
use crate::connection::{queue_message, CloseReason};
use minimium_environment::{
    time::ClockSample,
    {AstronomyModelRevision, CatalogueId, CatalogueSha256, EnvironmentDescriptor},
};
use minimium_protocol::ServerMessage;
use std::time::Instant;

const ENVIRONMENT_SYNC_INTERVAL: Duration = Duration::from_secs(5);

impl ServerRuntime {
    pub(super) fn environment_state_for_instance(
        &self,
        instance_id: minimium_world::world::WorldInstanceId,
        now: Instant,
    ) -> Result<(ClockSample, EnvironmentDescriptor), crate::environment::EnvironmentClockError>
    {
        let instance = self.game.worlds.get(instance_id).ok_or_else(|| {
            crate::environment::EnvironmentClockError::new(
                crate::environment::EnvironmentClockErrorKind::InvalidConfiguration,
                "environment world instance is unavailable",
            )
        })?;
        let identity = minimium_assets::embedded_sky_catalog_identity();
        let catalogue_id = CatalogueId::try_from(identity.catalogue_id).map_err(|error| {
            crate::environment::EnvironmentClockError::caused_by(
                crate::environment::EnvironmentClockErrorKind::InvalidConfiguration,
                "embedded sky catalogue identity is invalid",
                error,
            )
        })?;
        let descriptor = EnvironmentDescriptor::new(
            instance.clock_id,
            instance.environment_profile,
            AstronomyModelRevision::CURRENT,
            catalogue_id,
            CatalogueSha256::new(identity.sha256),
        );
        let monotonic = self.monotonic_ms(now).map_err(|error| {
            crate::environment::EnvironmentClockError::caused_by(
                crate::environment::EnvironmentClockErrorKind::InvalidConfiguration,
                "server monotonic clock failed",
                error,
            )
        })?;
        let sample = self.environment_clock.sample(monotonic)?;
        descriptor
            .validate_clock(sample.clock_id())
            .map_err(|error| {
                crate::environment::EnvironmentClockError::caused_by(
                    crate::environment::EnvironmentClockErrorKind::InvalidConfiguration,
                    "environment descriptor does not match clock",
                    error,
                )
            })?;
        Ok((sample, descriptor))
    }

    pub(super) fn replicate_environment(&mut self) {
        let now = Instant::now();
        let monotonic = match self.monotonic_ms(now) {
            Ok(monotonic) => monotonic,
            Err(error) => {
                crate::diagnostics::emit(
                    &self.environment_logger,
                    minimium_diagnostics::Level::Error,
                    "environment.replication_clock_failed",
                    &[("reason", error.to_string())],
                );
                return;
            }
        };
        let sample = match self.environment_clock.sample(monotonic) {
            Ok(sample) => sample,
            Err(error) => {
                crate::diagnostics::emit(
                    &self.environment_logger,
                    minimium_diagnostics::Level::Error,
                    "environment.replication_sample_failed",
                    &[
                        ("error_code", error.code().to_owned()),
                        ("reason", error.to_string()),
                    ],
                );
                return;
            }
        };
        let identity = minimium_assets::embedded_sky_catalog_identity();
        let catalogue_id = match CatalogueId::try_from(identity.catalogue_id) {
            Ok(catalogue_id) => catalogue_id,
            Err(error) => {
                crate::diagnostics::emit(
                    &self.environment_logger,
                    minimium_diagnostics::Level::Error,
                    "environment.catalogue_identity_invalid",
                    &[("reason", error.to_string())],
                );
                return;
            }
        };

        for index in 0..self.connections.clients.len() {
            if self.connections.disconnected.contains(&index)
                || self.connections.clients[index].close_deadline().is_some()
            {
                continue;
            }
            let Some(player) = self.connections.clients[index].player_id() else {
                continue;
            };
            let Some(context) = self.game.worlds.context_for_player(player) else {
                continue;
            };
            if self.connections.clients[index].last_environment_context != Some(context) {
                continue;
            }
            let Some(instance) = self.game.worlds.get(context.instance()) else {
                continue;
            };
            let descriptor = EnvironmentDescriptor::new(
                instance.clock_id,
                instance.environment_profile,
                AstronomyModelRevision::CURRENT,
                catalogue_id,
                CatalogueSha256::new(identity.sha256),
            );
            if let Err(error) = descriptor.validate_clock(sample.clock_id()) {
                crate::diagnostics::emit(
                    &self.environment_logger,
                    minimium_diagnostics::Level::Error,
                    "environment.sync_descriptor_invalid",
                    &[("reason", error.to_string())],
                );
                return;
            }
            let client = &self.connections.clients[index];
            let revision_changed = client.last_environment_revision != Some(sample.revision());
            let periodic_due = client.last_environment_sync_at.is_some_and(|last| {
                now.saturating_duration_since(last) >= ENVIRONMENT_SYNC_INTERVAL
            });
            if !revision_changed && !periodic_due {
                continue;
            }
            if !queue_message(
                &mut self.connections.clients[index],
                &mut self.metrics,
                ServerMessage::EnvironmentSync {
                    context,
                    clock_sample: sample,
                    environment: descriptor,
                },
            ) {
                self.metrics.dropped_messages = self.metrics.dropped_messages.saturating_add(1);
                self.connections.clients[index].disconnect_reason = Some(CloseReason::Backpressure);
                self.connections.disconnected.push(index);
                continue;
            }
            self.metrics.environment_sync_messages =
                self.metrics.environment_sync_messages.saturating_add(1);
            self.connections.clients[index].mark_environment_sent(context, sample.revision(), now);
        }
    }
}
