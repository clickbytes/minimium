use super::*;

impl DesktopHost {
    pub(super) fn handle_altitude_result(
        &mut self,
        context: minimium_protocol::WorldContext,
        position_mm: Option<minimium_world::world::Position3Mm>,
        update: Result<
            Option<crate::application::WorldContextChange>,
            crate::application::ApplicationMessageError,
        >,
    ) {
        if update.is_err() || self.application.core_view().context != Some(context) {
            return;
        }
        if position_mm.is_some() {
            self.renderer.exposure_history = None;
            self.refresh_after_position_change();
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
        update: Result<
            Option<crate::application::WorldContextChange>,
            crate::application::ApplicationMessageError,
        >,
    ) {
        if update.is_err() {
            return;
        }
        if position_mm.is_some() {
            self.renderer.exposure_history = None;
            self.meteors.audio.clear();
            let position = self.application.core_view().character.position;
            let origin = position.map(|coordinate| (coordinate / 256.0).round() * 256.0);
            self.renderer.set_render_origin(origin);
            self.shadow_state
                .invalidate(super::super::super::shadows::ShadowInvalidReason::StaleContext);
            self.application.set_loading(true);
            self.renderer.scene_visible = false;
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
            if !self.application.overview().is_open() && self.renderer.scene_visible {
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

    pub(super) fn handle_health_update(
        &mut self,
        health: u16,
        update: Result<
            Option<crate::application::WorldContextChange>,
            crate::application::ApplicationMessageError,
        >,
    ) {
        if update.is_err() {
            return;
        }
        if self.application.ui.console.open {
            self.application.ui.console.push_client_text(
                Text::Health { health },
                false,
                &self.localizer,
                self.application.settings.locale,
            );
        }
    }

    pub(super) fn handle_snapshot(
        &mut self,
        update: Result<
            Option<crate::application::WorldContextChange>,
            crate::application::ApplicationMessageError,
        >,
    ) {
        match update {
            Ok(_) => self.gameplay_presentation.remote_geometry_dirty = true,
            Err(crate::application::ApplicationMessageError::VisiblePlayerLimitExceeded) => {
                self.fail_connection(
                    Text::VisiblePlayerLimitExceeded
                        .localized(&self.localizer, self.application.settings.locale),
                );
            }
            Err(crate::application::ApplicationMessageError::StaleWorldContext) => {}
            Err(error) => {
                self.fail_connection(&format!("client snapshot update failed: {error:?}"))
            }
        }
    }
}
