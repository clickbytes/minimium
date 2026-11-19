use super::{actions, ClientApplicationCore};
use crate::{ClientTime, ClientTimeError, ControllerError, QueueError};
use minimium_protocol::{ClientMessage, WorldContext};
use minimium_world::world::{Position3Mm, WorldInstanceId};

impl ClientApplicationCore {
    pub(crate) fn poll_connector(
        &mut self,
        now: ClientTime,
    ) -> Result<crate::ConnectionEvent, ClientTimeError> {
        self.connection.poll_connector(now)
    }

    pub(crate) fn poll_admin_result(
        &mut self,
        now: ClientTime,
    ) -> Result<Option<minimium_commands::admin::AdminReplyV1>, ClientTimeError> {
        self.connection.poll_admin_result(now)
    }

    pub(crate) fn poll_session(
        &mut self,
        events: &mut Vec<crate::SessionEvent>,
        now: ClientTime,
    ) -> Result<(), ClientTimeError> {
        self.connection.poll_session(events, now)
    }

    pub(crate) fn clear_action_input_cooldown(&mut self) {
        self.action_executor.clear_input_cooldown();
    }

    pub fn start_remote(
        &mut self,
        address: String,
        now: ClientTime,
    ) -> Result<(), crate::connector::StartError> {
        self.accept_time(now)
            .map_err(crate::connector::StartError::Time)?;
        self.connection.start_remote(address, now)
    }

    pub fn start_local(
        &mut self,
        config: crate::LocalServerLaunchConfig,
        now: ClientTime,
    ) -> Result<(), crate::connector::StartError> {
        self.accept_time(now)
            .map_err(crate::connector::StartError::Time)?;
        self.connection.start_local(config, now)
    }

    pub fn cancel_connection_attempt(&mut self, now: ClientTime) -> Result<(), ClientTimeError> {
        self.accept_time(now)?;
        self.connection.cancel(now)
    }

    pub fn is_connection_busy(&self) -> bool {
        self.connection.busy()
    }

    pub fn record_diagnostic_failure(&self, code: &'static str, error: &impl std::fmt::Display) {
        self.connection.record_diagnostic_failure(code, error);
    }

    pub fn execute_action(
        &mut self,
        action: actions::ClientAction,
        now: ClientTime,
    ) -> actions::LocalActionOutcome {
        let map_teleport = matches!(action, actions::ClientAction::MapTeleport { .. });
        if let Err(error) = self.accept_time(now) {
            return actions::LocalActionOutcome::Rejected {
                kind: actions::QueuedActionKind::Other,
                error: QueueError::Time(error),
            };
        }
        if map_teleport && self.map_teleport_pending {
            return actions::LocalActionOutcome::Suppressed(
                actions::SuppressionReason::MapTeleportPending,
            );
        }
        let outcome = self
            .action_executor
            .execute(&mut self.connection, action, now);
        if map_teleport && matches!(outcome, actions::LocalActionOutcome::Queued { .. }) {
            self.map_teleport_pending = true;
        }
        outcome
    }

    pub fn transfer_cpu_content_ready(&self, ticket: crate::TransferTicket) -> bool {
        let Some(preparation) = self.pending_transfer else {
            return false;
        };
        if preparation.ticket != ticket {
            return false;
        }
        let Some(session) = self.connection.session() else {
            return false;
        };
        if session.active_world_context() != Some(preparation.source_context) {
            return false;
        }
        let Some(content) = session.resolved_world_content_for_context(ticket.context()) else {
            return false;
        };
        if content.manifest() != preparation.manifest
            || content.map().fingerprint() != preparation.map_fingerprint
        {
            return false;
        }
        let Some(terrain) = session.terrain_inputs_for_context(ticket.context()) else {
            return false;
        };
        if preparation
            .manifest
            .planet()
            .is_some_and(|planet| planet.mode == 1)
            && terrain.source().geography().is_none()
        {
            return false;
        }
        preparation.manifest.planet().is_none()
            || session
                .water_catalog_for_context(ticket.context())
                .is_some()
    }

    pub fn finish_until(
        &mut self,
        deadline: std::time::Instant,
        now: ClientTime,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.accept_time(now)?;
        self.connection.finish_until(deadline, now)
    }

    pub fn disconnect_session(
        &mut self,
        now: ClientTime,
    ) -> Result<Option<crate::ClientSession>, ClientTimeError> {
        self.accept_time(now)?;
        let session = self.connection.take_session(now)?;
        self.game.set_session_context(None);
        self.pending_transfer = None;
        self.bootstrap_terrain = None;
        self.game.gameplay.clear_remote_players();
        self.game.gameplay.health = minimium_simulation::PLAYER_MAX_HEALTH;
        self.game.gameplay.sectors_ready = false;
        self.game.gameplay.collision_index = minimium_simulation::CollisionIndex::default();
        self.game.gameplay.collision_revision =
            self.game.gameplay.collision_revision.wrapping_add(1).max(1);
        self.action_executor.clear_input_cooldown();
        self.map_teleport_pending = false;
        Ok(session)
    }

    /// Host supplies a freshly prepared offline source after a disconnect;
    /// the core accepts only exact current-world identities.
    pub fn provide_bootstrap_terrain(
        &mut self,
        terrain: Option<minimium_procgen::TerrainInputs>,
        now: ClientTime,
    ) -> Result<(), QueueError> {
        self.accept_time(now).map_err(QueueError::Time)?;
        if self.game.session_context().is_some() || self.connection.session().is_some() {
            return Err(QueueError::StaleWorldContext);
        }
        if let Some(terrain) = terrain.as_ref() {
            let instance = self.game.current_instance;
            let Some(manifest) = self.game.world_manifests.get(&instance).copied() else {
                return Err(QueueError::StaleWorldContext);
            };
            let Some(map) = self.world_catalog.get(&instance) else {
                return Err(QueueError::StaleWorldContext);
            };
            if !terrain.source().matches_map(instance, manifest, map)
                || terrain.overlays().world() != Some(instance)
            {
                return Err(QueueError::StaleWorldContext);
            }
        }
        self.bootstrap_terrain = terrain;
        Ok(())
    }

    pub(crate) fn apply_altitude_result(
        &mut self,
        context: WorldContext,
        position: Option<Position3Mm>,
    ) -> bool {
        if self.game.session_context() != Some(context)
            || self
                .connection
                .session()
                .is_none_or(|s| s.active_world_context() != Some(context))
        {
            return false;
        }
        if let Some(position) = position {
            let prediction = &mut self.game.gameplay.prediction;
            prediction.position_mm = Some(position);
            prediction.character.position = position.0.map(|v| v as f32 / 1000.0);
            prediction.character.velocity = [0.0; 3];
            prediction.character.grounded = false;
            prediction.reset();
            self.game.gameplay.sectors_ready = false;
        }
        true
    }

    pub(crate) fn apply_health_update(&mut self, health: u16) -> bool {
        let Some(context) = self.game.session_context() else {
            return false;
        };
        if self
            .connection
            .session()
            .is_none_or(|s| s.active_world_context() != Some(context))
        {
            return false;
        }
        self.game.gameplay.health = health;
        true
    }

    pub fn map_teleport_pending(&self) -> bool {
        self.map_teleport_pending
    }

    pub(crate) fn apply_map_teleport_position(&mut self, position: Option<Position3Mm>) -> bool {
        let Some(context) = self.game.session_context() else {
            return false;
        };
        if !self.map_teleport_pending
            || self
                .connection
                .session()
                .is_none_or(|s| s.active_world_context() != Some(context))
        {
            return false;
        }
        self.map_teleport_pending = false;
        if let Some(position) = position {
            self.game.gameplay.prediction.reset();
            self.game.gameplay.prediction.position_mm = Some(position);
            self.game.gameplay.prediction.character.position =
                position.0.map(|v| v as f32 / 1000.0);
            self.game.gameplay.prediction.character.velocity = [0.0; 3];
            self.game.gameplay.prediction.character.grounded = false;
            self.game.gameplay.prediction.reset();
            self.game.gameplay.sectors_ready = false;
        }
        true
    }

    pub fn interpolate_remote_players(
        &mut self,
        now: ClientTime,
        delay: std::time::Duration,
    ) -> Result<bool, ClientTimeError> {
        self.accept_time(now)?;
        if let Some(ms) = self
            .connection
            .session()
            .and_then(|s| s.environment_frame().simulation_ms)
        {
            return self.game.gameplay.reconciliation.interpolate_at(
                now,
                ms,
                &mut self.game.gameplay.remote_positions_mm,
            );
        }
        self.game.gameplay.reconciliation.interpolate_positions(
            now,
            delay,
            &mut self.game.gameplay.remote_positions_mm,
        )
    }

    pub fn invalidate_active_scene(
        &mut self,
        context: Option<WorldContext>,
        instance: WorldInstanceId,
        now: ClientTime,
    ) -> Result<bool, ClientTimeError> {
        self.accept_time(now)?;
        if self.game.session_context() == context && self.game.current_instance == instance {
            self.game.gameplay.sectors_ready = false;
            return Ok(true);
        }
        Ok(false)
    }

    pub fn cancel_transition_request(&mut self, now: ClientTime) -> Result<(), ClientTimeError> {
        self.accept_time(now)?;
        self.connection.cancel_transition(now).map(|_| ())
    }

    pub fn request_pending_transition_cancel(
        &mut self,
        now: ClientTime,
    ) -> Result<bool, ClientTimeError> {
        self.accept_time(now)?;
        self.connection.cancel_transition(now)
    }

    pub fn report_sky_view(
        &mut self,
        context: WorldContext,
        direction: [f32; 3],
        observation: minimium_protocol::SkyObservation,
        now: ClientTime,
    ) -> Result<bool, ClientTimeError> {
        self.accept_time(now)?;
        if self.game.session_context() != Some(context) {
            return Ok(false);
        }
        Ok(self.connection.session_mut().is_some_and(|s| {
            s.queue(
                &ClientMessage::SkyView {
                    context,
                    direction,
                    observation,
                },
                now,
            )
            .is_ok()
        }))
    }

    pub fn request_clock(
        &mut self,
        change: minimium_environment::clock_command::ClockWrite,
        now: ClientTime,
    ) -> Result<minimium_commands::admin::Correlation, ControllerError> {
        self.accept_controller_time(now)?;
        self.connection.request_clock(change, now)
    }
    pub fn request_set_time(
        &mut self,
        minute: u16,
        longitude: f64,
        now: ClientTime,
    ) -> Result<(), ControllerError> {
        self.accept_controller_time(now)?;
        self.connection.request_set_time(minute, longitude, now)
    }
    pub fn request_initial_clock_config(
        &mut self,
        now: ClientTime,
    ) -> Result<minimium_commands::admin::Correlation, ControllerError> {
        self.accept_controller_time(now)?;
        self.connection.request_initial_clock_config(now)
    }
    pub fn request_admin_help(
        &mut self,
        target: &str,
        now: ClientTime,
    ) -> Result<minimium_commands::admin::Correlation, ControllerError> {
        self.accept_controller_time(now)?;
        self.connection.request_admin_help(target, now)
    }
    pub fn request_time_scale(
        &mut self,
        value: String,
        now: ClientTime,
    ) -> Result<(), ControllerError> {
        self.accept_controller_time(now)?;
        self.connection.request_time_scale(value, now)
    }
    pub fn request_time_mode(
        &mut self,
        mode: &str,
        now: ClientTime,
    ) -> Result<(), ControllerError> {
        self.accept_controller_time(now)?;
        self.connection.request_time_mode(mode, now)
    }
    pub fn request_time_offline_policy(
        &mut self,
        policy: &str,
        now: ClientTime,
    ) -> Result<(), ControllerError> {
        self.accept_controller_time(now)?;
        self.connection.request_time_offline_policy(policy, now)
    }
    pub fn request_random_bolid(
        &mut self,
        count: u16,
        now: ClientTime,
    ) -> Result<(), ControllerError> {
        self.accept_controller_time(now)?;
        self.connection.request_random_bolid(count, now)
    }
    pub fn request_random_meteorid(&mut self, now: ClientTime) -> Result<(), ControllerError> {
        self.accept_controller_time(now)?;
        self.connection.request_random_meteorid(now)
    }

    pub fn request_bolid_shower_status(
        &mut self,
        id: u64,
        now: ClientTime,
    ) -> Result<(), ControllerError> {
        self.accept_controller_time(now)?;
        self.connection.request_bolid_shower_status(id, now)
    }

    pub fn request_impact_receipt_status(
        &mut self,
        world: minimium_world::world::WorldInstanceId,
        id: u64,
        now: ClientTime,
    ) -> Result<(), ControllerError> {
        self.accept_controller_time(now)?;
        self.connection
            .request_impact_receipt_status(world, id, now)
    }

    pub fn request_persistence_save(&mut self, now: ClientTime) -> Result<(), ControllerError> {
        self.accept_controller_time(now)?;
        self.connection.request_persistence_save(now)
    }

    pub fn request_persistence_status(
        &mut self,
        id: Option<u128>,
        now: ClientTime,
    ) -> Result<(), ControllerError> {
        self.accept_controller_time(now)?;
        self.connection.request_persistence_status(id, now)
    }

    pub fn take_material_package_if_awaiting_gpu(
        &mut self,
        now: ClientTime,
    ) -> Result<Option<Option<minimium_asset_io::materials::MaterialPackage>>, ClientTimeError>
    {
        self.accept_time(now)?;
        let Some(session) = self.connection.session_mut() else {
            return Ok(None);
        };
        Ok(session
            .content_awaiting_gpu()
            .then(|| session.take_material_package()))
    }

    pub fn finish_material_content(&mut self, now: ClientTime) -> Result<Option<()>, QueueError> {
        self.accept_time(now).map_err(QueueError::Time)?;
        self.connection
            .session_mut()
            .map(|s| s.finish_content(now))
            .transpose()
    }
}
