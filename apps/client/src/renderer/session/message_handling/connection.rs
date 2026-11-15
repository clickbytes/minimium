use super::*;

impl Renderer {
    pub(super) fn handle_welcome(&mut self, message: ServerMessage) {
        let ServerMessage::Welcome {
            id,
            map_fingerprint,
            context,
            health,
            position,
            position_mm,
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
        if !self.load_resolved_catalog(context.instance(), manifest) {
            self.fail_connection("server generation manifest is not supported");
            return;
        }
        if !self.install_world(
            context.instance(),
            map_fingerprint,
            Some(position),
            manifest,
        ) {
            self.fail_connection("saved world instance is not available on this client");
            return;
        }
        self.application.game.set_session_context(Some(context));
        self.application.game.gameplay.health = health;
        self.gpu.scene_visible = !self.application.is_loading();
        self.application.status =
            crate::application::ClientStatus::Status(if self.application.settings.require_earth {
                minimium_localization::UiStatus::EarthConnected
            } else {
                minimium_localization::UiStatus::Connected
            });
        self.application.game.gameplay.prediction.position_mm =
            manifest.planet().map(|_| position_mm);
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
