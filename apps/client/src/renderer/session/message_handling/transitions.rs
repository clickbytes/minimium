use super::*;

impl Renderer {
    pub(super) fn handle_transition_prepare(
        &mut self,
        transition_id: minimium_world::world::TransitionId,
        source_context: minimium_protocol::WorldContext,
        target_context: minimium_protocol::WorldContext,
        map_fingerprint: u64,
        destination: [f32; 3],
        manifest: WorldManifest,
    ) {
        let instance = target_context.instance();
        if !self.load_resolved_catalog(instance, manifest) {
            self.fail_connection("target generation manifest is not supported");
            return;
        }
        let matches_catalog = self
            .application
            .world_catalog
            .get(&instance)
            .is_some_and(|map| map.fingerprint() == map_fingerprint)
            && self.application.game.world_manifests.get(&instance) == Some(&manifest);
        if !matches_catalog {
            self.fail_connection("target world content does not match the loaded manifest");
            return;
        }
        if !self.application.register_transition(
            transition_id,
            PendingSectorTransition::preparing(
                source_context,
                target_context,
                destination,
                manifest,
            ),
        ) {
            self.fail_connection("duplicate world transfer ticket");
            return;
        }
        if self.application.ui.console.open {
            self.application.ui.console.push_ui_message(
                minimium_localization::UiMessage::WorldTransferPrepared,
                &self.localizer,
                self.application.settings.locale,
            );
        }
        self.advance_pending_transitions();
    }

    pub(super) fn handle_transition_committed(
        &mut self,
        transition_id: minimium_world::world::TransitionId,
        context: minimium_protocol::WorldContext,
        map_fingerprint: u64,
        destination: [f32; 3],
        destination_mm: minimium_world::world::Position3Mm,
        health: u16,
        manifest: WorldManifest,
    ) {
        if !self
            .application
            .can_commit_transition(transition_id, context)
        {
            self.fail_connection("world transfer commit has no matching ready ticket");
            return;
        }
        if !self.load_resolved_catalog(context.instance(), manifest) {
            self.fail_connection("target generation manifest is not supported");
            return;
        }
        if !self.install_world(
            context.instance(),
            map_fingerprint,
            Some(destination),
            manifest,
        ) {
            self.fail_connection("target world geometry does not match the server");
            return;
        }
        if !self.application.commit_transition(transition_id, context) {
            self.fail_connection("world transfer ticket changed during commit");
            return;
        }
        self.application.game.set_session_context(Some(context));
        self.application.game.gameplay.health = health;
        self.application.game.gameplay.prediction.position_mm =
            manifest.planet().map(|_| destination_mm);
        if self.application.ui.console.open {
            self.application.ui.console.push_ui_message(
                minimium_localization::UiMessage::WorldTransferCommitted,
                &self.localizer,
                self.application.settings.locale,
            );
        }
    }

    pub(super) fn handle_transition_cancelled(
        &mut self,
        transition_id: minimium_world::world::TransitionId,
        context: minimium_protocol::WorldContext,
    ) {
        if let Some(pending) = self.application.cancel_transition(transition_id, context) {
            self.release_prepared_target(pending.context.instance());
        }
    }
}
