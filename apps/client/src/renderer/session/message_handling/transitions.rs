use super::*;

impl DesktopHost {
    pub(super) fn handle_transition_prepare(
        &mut self,
        transition_id: minimium_world::world::TransitionId,
        source_context: minimium_protocol::WorldContext,
        target_context: minimium_protocol::WorldContext,
        map_fingerprint: u64,
        destination: [f32; 3],
        manifest: WorldManifest,
    ) {
        if self.application.has_transition_id(transition_id) {
            return;
        }
        let Some(map) =
            self.application
                .resolved_map_for_context(target_context, map_fingerprint, manifest)
        else {
            self.fail_connection("target generation manifest is not supported");
            return;
        };
        if map.fingerprint() != map_fingerprint {
            self.fail_connection("target world content does not match the loaded manifest");
            return;
        }
        if !self.application.register_transition(
            transition_id,
            PendingSectorTransition::preparing(
                transition_id,
                source_context,
                target_context,
                destination,
                manifest,
                map,
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
    }

    pub(super) fn handle_transition_committed(
        &mut self,
        transition_id: minimium_world::world::TransitionId,
        context: minimium_protocol::WorldContext,
        update: Result<
            Option<crate::application::WorldContextChange>,
            crate::application::ApplicationMessageError,
        >,
    ) {
        if !self.application.has_transition(transition_id, context) {
            self.fail_connection("world transfer commit has no matching ready ticket");
            return;
        }
        let change = match update {
            Ok(Some(change)) => change,
            Err(error) => {
                self.fail_connection(&format!("client transfer state update failed: {error:?}"));
                return;
            }
            Ok(None) => {
                self.fail_connection("transfer commit did not update the active world");
                return;
            }
        };
        if !self.install_world(
            super::super::super::sector_scene::SceneInterestOwner::Pending(transition_id, context),
            change,
        ) {
            self.fail_connection("target world geometry does not match the server");
            return;
        }
        if !self.application.commit_transition(transition_id, context) {
            self.fail_connection("world transfer ticket changed during commit");
            return;
        }
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
            self.release_prepared_target(transition_id, pending.context);
        }
    }
}
