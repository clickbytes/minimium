use super::*;

impl Renderer {
    pub(in crate::renderer) fn update_remote_player_rendering(&mut self, now: Instant) {
        if self.application.connection.session().is_none() {
            self.application.action_executor.clear_input_cooldown();
            self.application.game.gameplay.clear_remote_players();
            self.gameplay_presentation.clear_remote_players();
            self.gpu.clear_remote_vertices();
            self.gpu.update_remote_shadow_vertices(&[], None);
            return;
        }
        let stamp = self.application.game.session_context().and_then(|context| {
            let session = self.application.connection.session()?;
            let publication = self.gpu.world_geometry.publication()?.stamp;
            (self.gpu.scene_visible
                && session.active_world_context() == Some(context)
                && publication.instance == context.instance()
                && publication.content_revision == context.content_revision())
            .then_some(RemotePlayerGeometryStamp {
                context,
                publication,
            })
        });
        let Some(stamp) = stamp else {
            self.gpu.clear_remote_vertices();
            self.gpu.update_remote_shadow_vertices(&[], None);
            return;
        };
        // A new world, membership, publication or origin needs new bytes even
        // when no remote player has moved since the preceding frame.
        if self.gpu.remote_stamp != Some(stamp) {
            self.gameplay_presentation.remote_geometry_dirty = true;
        }
        if self.gameplay_presentation.update_remote_vertices(
            &mut self.application.game.gameplay,
            now,
            self.gpu.render_origin,
        ) {
            self.gpu
                .update_remote_vertices(&self.gameplay_presentation.remote_vertices, stamp);
        }
        let center = self
            .application
            .game
            .gameplay
            .prediction
            .position_mm
            .map_or_else(
                || {
                    self.application
                        .game
                        .gameplay
                        .prediction
                        .character
                        .position
                        .map(f64::from)
                },
                |position| position.0.map(|value| value as f64 / 1000.0),
            );
        let shadow_vertices_changed = self.gameplay_presentation.update_remote_shadow_vertices(
            &self.application.game.gameplay,
            center,
            self.gpu.render_origin,
        );
        let geometry_stamp = Some(stamp.publication);
        // Even an unchanged (or empty) player batch must acknowledge a new
        // terrain publication. Otherwise the shadow frame stays incomplete
        // until a remote player happens to move.
        if self.gpu.shadow_remote_dirty
            || shadow_vertices_changed
            || self.gpu.shadow_remote_stamp != geometry_stamp
        {
            self.gpu.update_remote_shadow_vertices(
                &self.gameplay_presentation.remote_shadow_vertices,
                geometry_stamp,
            );
        }
    }
}
