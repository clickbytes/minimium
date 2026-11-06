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
        if !self.load_generated_catalog(instance, manifest) {
            self.fail_connection("target generation manifest is not supported");
            return;
        }
        let matches_catalog = self
            .world_catalog
            .get(&instance)
            .is_some_and(|map| map.fingerprint() == map_fingerprint)
            && self.game.world_manifests.get(&instance) == Some(&manifest);
        if !matches_catalog {
            self.fail_connection("target world content does not match the loaded manifest");
            return;
        }
        self.pending_sector_transitions.insert(
            transition_id,
            PendingSectorTransition {
                source_context,
                context: target_context,
                destination,
                manifest,
                ready_sent: false,
            },
        );
        if self.ui.console.open {
            self.ui.console.push_line("Preparing world transfer");
        }
        self.advance_pending_transitions();
    }

    pub(super) fn handle_transition_committed(
        &mut self,
        context: minimium_protocol::WorldContext,
        map_fingerprint: u64,
        destination: [f32; 3],
        destination_mm: minimium_world::world::Position3Mm,
        health: u16,
        manifest: WorldManifest,
    ) {
        if !self.load_generated_catalog(context.instance(), manifest) {
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
        self.game.session_context = Some(context);
        self.game.gameplay.health = health;
        self.game.gameplay.prediction.position_mm = manifest.planet().map(|_| destination_mm);
        if self.ui.console.open {
            self.ui.console.push_line("World transfer committed");
        }
    }

    pub(super) fn handle_transition_cancelled(
        &mut self,
        transition_id: minimium_world::world::TransitionId,
        context: minimium_protocol::WorldContext,
    ) {
        if let Some(pending) = self
            .pending_sector_transitions
            .get(&transition_id)
            .copied()
            .filter(|pending| pending.source_context == context)
        {
            self.pending_sector_transitions.remove(&transition_id);
            self.release_prepared_target(pending.context.instance());
        }
    }
}
