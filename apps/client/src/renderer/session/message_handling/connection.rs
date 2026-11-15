use super::*;

impl DesktopHost {
    pub(super) fn handle_welcome(&mut self, message: ServerMessage) {
        let game_message = message.clone();
        let ServerMessage::Welcome {
            id,
            context,
            health,
            manifest,
            ..
        } = message
        else {
            return;
        };
        if self.application.settings.require_earth
            && manifest.planet().is_none_or(|planet| planet.mode != 1)
        {
            self.fail_connection("serwer nie udostępnia manifestu Earth");
            return;
        }
        let change = match self.application.apply_world_message(&game_message) {
            Ok(change) => change,
            Err(error) => {
                self.fail_connection(&format!("server world content is not supported: {error:?}"));
                return;
            }
        };
        let bootstrap_instance = change.previous_instance;
        if !self.install_world(
            super::super::super::sector_scene::SceneInterestOwner::Active(context),
            change,
        ) {
            self.fail_connection("saved world instance is not available on this client");
            return;
        }
        let bootstrap =
            super::super::super::sector_scene::SceneInterestOwner::Bootstrap(bootstrap_instance);
        self.sector_cache.release_interest(bootstrap);
        self.renderer
            .release_scene_stage(bootstrap_instance, bootstrap);
        if bootstrap_instance != context.instance() {
            self.release_world_sectors(bootstrap_instance);
        }
        self.renderer.scene_visible = !self.application.is_loading();
        self.application.status =
            crate::application::ClientStatus::Status(if self.application.settings.require_earth {
                minimium_localization::UiStatus::EarthConnected
            } else {
                minimium_localization::UiStatus::Connected
            });
        if self.application.ui.console.open {
            self.application.ui.console.push_client_text(
                Text::Connected { id, health },
                false,
                &self.localizer,
                self.application.settings.locale,
            );
        }
    }
}
