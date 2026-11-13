use super::*;

impl Renderer {
    pub(super) fn handle_altitude_result(
        &mut self,
        context: minimium_protocol::WorldContext,
        position_mm: Option<minimium_world::world::Position3Mm>,
    ) {
        if Some(context) != self.application.game.session_context() {
            return;
        }
        if let Some(position) = position_mm {
            self.apply_altitude_position(position);
            self.application.ui.console.push_ui_message(
                minimium_localization::UiMessage::AltitudeResult(true),
                &self.localizer,
                self.application.settings.locale,
            );
        } else {
            self.application.ui.console.push_ui_error_message(
                minimium_localization::UiMessage::AltitudeResult(false),
                &self.localizer,
                self.application.settings.locale,
            );
        }
    }

    pub(super) fn handle_map_teleport_result(
        &mut self,
        position_mm: Option<minimium_world::world::Position3Mm>,
    ) {
        self.application.resolve_map_teleport();
        if let Some(position_mm) = position_mm {
            self.meteors.audio.clear();
            self.application.game.gameplay.prediction.reset();
            self.application.game.gameplay.prediction.position_mm = Some(position_mm);
            self.application.game.gameplay.prediction.character.position =
                position_mm.0.map(|coordinate| coordinate as f32 / 1000.0);
            let origin = self
                .application
                .game
                .gameplay
                .prediction
                .character
                .position
                .map(|coordinate| (coordinate / 256.0).round() * 256.0);
            self.gpu.set_render_origin(origin);
            self.shadow_state
                .invalidate(super::super::super::shadows::ShadowInvalidReason::StaleContext);
            self.application.game.gameplay.prediction.character.velocity = [0.0; 3];
            self.application.game.gameplay.prediction.character.grounded = false;
            self.application.game.gameplay.sectors_ready = false;
            self.application.set_loading(true);
            self.gpu.scene_visible = false;
            self.last_render_position = [f32::INFINITY; 3];
            self.application.input.clear_held();
            self.application.status = crate::application::ClientStatus::Status(
                minimium_localization::UiStatus::EarthLoading,
            );
            self.application.ui.console.push_ui_message(
                minimium_localization::UiMessage::MapTeleportResult(true),
                &self.localizer,
                self.application.settings.locale,
            );
        } else {
            self.application.status = crate::application::ClientStatus::Status(
                minimium_localization::UiStatus::EarthChooseLand,
            );
            self.application.ui.console.push_ui_error_message(
                minimium_localization::UiMessage::MapTeleportResult(false),
                &self.localizer,
                self.application.settings.locale,
            );
            if !self.application.overview().is_open() && self.gpu.scene_visible {
                self.toggle_overview();
                self.application.arm_map_teleport();
            }
        }
    }

    pub(super) fn handle_shot_result(
        &mut self,
        target_id: Option<minimium_core::ids::PlayerId>,
        remaining_health: Option<u16>,
        killed: bool,
    ) {
        if target_id.is_some() {
            self.gameplay_presentation.hit_confirmation_until =
                Some(Instant::now() + Duration::from_millis(180));
        }
        if self.application.ui.console.open {
            match (target_id, remaining_health, killed) {
                (Some(target_id), Some(health), true) => {
                    self.application.ui.console.push_client_text(
                        Text::TargetRespawned { target_id, health },
                        false,
                        &self.localizer,
                        self.application.settings.locale,
                    )
                }
                (Some(target_id), Some(health), false) => {
                    self.application.ui.console.push_client_text(
                        Text::Hit { target_id, health },
                        false,
                        &self.localizer,
                        self.application.settings.locale,
                    )
                }
                _ => self.application.ui.console.push_client_text(
                    Text::ShotMissed,
                    false,
                    &self.localizer,
                    self.application.settings.locale,
                ),
            }
        }
    }

    pub(super) fn handle_health_update(&mut self, health: u16) {
        self.application.game.gameplay.health = health;
        if self.application.ui.console.open {
            self.application.ui.console.push_client_text(
                Text::Health { health },
                false,
                &self.localizer,
                self.application.settings.locale,
            );
        }
    }

    pub(super) fn handle_snapshot(&mut self, message: ServerMessage) {
        let ServerMessage::Snapshot {
            server_time_ms,
            server_tick,
            full_snapshot,
            resolved_input_seq,
            self_position,
            self_position_mm,
            self_velocity,
            self_grounded,
            self_movement,
            players,
            removed_players,
            ..
        } = message
        else {
            return;
        };
        if !self.application.game.gameplay.apply_snapshot(
            server_time_ms,
            full_snapshot,
            server_tick,
            resolved_input_seq,
            self_position,
            self_position_mm,
            self_velocity,
            self_grounded,
            self_movement,
            self.application.connection.movement,
            self.application
                .game
                .world_manifests
                .get(&self.application.game.current_instance)
                .copied(),
            self.application
                .connection
                .session()
                .and_then(|session| session.prepared_geography()),
            &players,
            &removed_players,
        ) {
            self.fail_connection(
                Text::VisiblePlayerLimitExceeded
                    .localized(&self.localizer, self.application.settings.locale),
            );
        } else {
            self.gameplay_presentation.remote_geometry_dirty = true;
        }
    }
}
