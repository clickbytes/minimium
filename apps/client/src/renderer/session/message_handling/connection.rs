use super::*;

impl Renderer {
    pub(super) fn handle_welcome(
        &mut self,
        id: minimium_core::ids::PlayerId,
        map_fingerprint: u64,
        context: minimium_protocol::WorldContext,
        health: u16,
        position: [f32; 3],
        position_mm: minimium_world::world::Position3Mm,
        manifest: WorldManifest,
    ) {
        if self.settings.require_earth && manifest.planet().is_none_or(|planet| planet.mode != 1) {
            self.fail_connection("serwer nie udostępnia manifestu Earth");
            return;
        }
        if !self.load_generated_catalog(context.instance(), manifest) {
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
        self.game.session_context = Some(context);
        self.game.gameplay.health = health;
        self.gpu.scene_visible = !self.teleport_loading;
        self.connection_status = if self.settings.require_earth {
            "Earth: połączono".into()
        } else {
            "Połączono".into()
        };
        self.game.gameplay.prediction.position_mm = manifest.planet().map(|_| position_mm);
        if self.ui.console.open {
            self.ui
                .console
                .push_line(Text::Connected { id, health }.to_string());
        }
    }
}
