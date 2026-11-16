use super::ClientApplication;

impl ClientApplication {
    pub(crate) fn retain_terminal_readiness_interests(
        &self,
        readiness: &mut minimium_client_runtime::world_readiness::WorldReadiness,
    ) -> Result<(), minimium_client_runtime::world_readiness::WorldReadinessError> {
        let instance = self.game.current_instance;
        let active = self.game.session_context().map_or_else(
            || minimium_client_runtime::world_readiness::WorldReadinessOwner::Bootstrap(instance),
            minimium_client_runtime::world_readiness::WorldReadinessOwner::Active,
        );
        let mut owners = vec![active];
        if let Some(preparation) = self.pending_transfer {
            owners.push(
                minimium_client_runtime::world_readiness::WorldReadinessOwner::Pending(
                    preparation.ticket.transition_id(),
                    preparation.ticket.context(),
                ),
            );
        }
        readiness.retain_interests(&owners)
    }

    pub(crate) fn report_transfer_host_ready(
        &mut self,
        ticket: minimium_client_runtime::TransferTicket,
        snapshot: minimium_client_runtime::world_readiness::CollisionSnapshot,
        now: minimium_client_runtime::ClientTime,
    ) -> Result<(), crate::session::QueueError> {
        self.accept_time(now)
            .map_err(crate::session::QueueError::Time)?;
        if !self.transfer_cpu_content_ready(ticket) {
            return Err(crate::session::QueueError::StaleWorldContext);
        }
        let Some(preparation) = self.pending_transfer else {
            return Err(crate::session::QueueError::StaleWorldContext);
        };
        let Some(session) = self.connection.session() else {
            return Err(crate::session::QueueError::StaleWorldContext);
        };
        let Some(content) = session.resolved_world_content_for_context(ticket.context()) else {
            return Err(crate::session::QueueError::StaleWorldContext);
        };
        let Some(terrain) = session.terrain_inputs_for_context(ticket.context()) else {
            return Err(crate::session::QueueError::StaleWorldContext);
        };
        let owner = minimium_client_runtime::world_readiness::WorldReadinessOwner::Pending(
            ticket.transition_id(),
            ticket.context(),
        );
        if snapshot.owner() != owner
            || snapshot.position() != preparation.destination
            || !snapshot.matches(owner, preparation.manifest, content.map(), &terrain)
            || !snapshot.covers_position(preparation.destination)
        {
            return Err(crate::session::QueueError::StaleWorldContext);
        }
        if session.host_readiness_requested(ticket) {
            return Ok(());
        }
        self.connection.report_transfer_host_ready(ticket, now)
    }

    pub(crate) fn ensure_active_collision(
        &mut self,
        readiness: &mut minimium_client_runtime::world_readiness::WorldReadiness,
    ) -> Result<Option<minimium_client_runtime::world_readiness::CollisionSnapshot>, String> {
        let instance = self.game.current_instance;
        let owner = self.game.session_context().map_or_else(
            || minimium_client_runtime::world_readiness::WorldReadinessOwner::Bootstrap(instance),
            minimium_client_runtime::world_readiness::WorldReadinessOwner::Active,
        );
        let manifest = self
            .game
            .world_manifests
            .get(&instance)
            .copied()
            .ok_or_else(|| "active world manifest is unavailable".to_owned())?;
        let map = self
            .world_catalog
            .get(&instance)
            .ok_or_else(|| "active world map is unavailable".to_owned())?;
        let position = self.game.gameplay.prediction.character.position;
        let terrain = if let Some(context) = owner.context() {
            self.connection
                .session()
                .and_then(|session| session.terrain_inputs_for_context(context))
        } else {
            crate::world_content::bootstrap_terrain_inputs(instance, manifest, map).ok()
        };
        let Some(terrain) = terrain else {
            return Ok(None);
        };
        let snapshot = readiness
            .ensure_collision(owner, manifest, position, map, terrain)
            .map_err(|error| error.to_string())?;
        if let Some(snapshot) = snapshot.as_ref() {
            let _ = self.report_active_scene_ready(snapshot.clone());
        }
        Ok(snapshot)
    }

    pub(crate) fn ensure_pending_transfer_collision(
        &mut self,
        readiness: &mut minimium_client_runtime::world_readiness::WorldReadiness,
    ) -> Result<
        Option<(
            minimium_client_runtime::TransferTicket,
            minimium_client_runtime::world_readiness::CollisionSnapshot,
        )>,
        String,
    > {
        let Some(preparation) = self.pending_transfer else {
            return Ok(None);
        };
        if self.connection.host_readiness_requested(preparation.ticket) {
            return Ok(None);
        }
        let Some(session) = self.connection.session() else {
            return Ok(None);
        };
        let Some(content) =
            session.resolved_world_content_for_context(preparation.ticket.context())
        else {
            return Ok(None);
        };
        let Some(terrain) = session.terrain_inputs_for_context(preparation.ticket.context()) else {
            return Ok(None);
        };
        if content.manifest() != preparation.manifest
            || content.map().fingerprint() != preparation.map_fingerprint
        {
            return Ok(None);
        }
        let owner = minimium_client_runtime::world_readiness::WorldReadinessOwner::Pending(
            preparation.ticket.transition_id(),
            preparation.ticket.context(),
        );
        let snapshot = readiness
            .ensure_collision(
                owner,
                preparation.manifest,
                preparation.destination,
                content.map(),
                terrain,
            )
            .map_err(|error| error.to_string())?;
        Ok(snapshot.map(|snapshot| (preparation.ticket, snapshot)))
    }

    pub(crate) fn report_active_scene_ready(
        &mut self,
        snapshot: minimium_client_runtime::world_readiness::CollisionSnapshot,
    ) -> bool {
        let instance = self.game.current_instance;
        let owner = self.game.session_context().map_or_else(
            || minimium_client_runtime::world_readiness::WorldReadinessOwner::Bootstrap(instance),
            minimium_client_runtime::world_readiness::WorldReadinessOwner::Active,
        );
        let Some(manifest) = self.game.world_manifests.get(&instance).copied() else {
            return false;
        };
        let Some(map) = self.world_catalog.get(&instance) else {
            return false;
        };
        let terrain = if let Some(context) = owner.context() {
            self.connection
                .session()
                .and_then(|session| session.terrain_inputs_for_context(context))
        } else {
            crate::world_content::bootstrap_terrain_inputs(instance, manifest, map).ok()
        };
        let Some(terrain) = terrain else {
            return false;
        };
        if !snapshot.matches(owner, manifest, map, &terrain)
            || !snapshot.covers_position(self.game.gameplay.prediction.character.position)
        {
            return false;
        }
        self.game.gameplay.collision_index = snapshot.collision().clone();
        self.game.gameplay.collision_revision = snapshot.collision_revision();
        self.game.gameplay.sectors_ready = true;
        true
    }
}
