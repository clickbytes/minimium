use super::*;

mod connection;
mod gameplay;
mod transitions;

impl Renderer {
    pub(super) fn handle_server_message(&mut self, message: ServerMessage) {
        match message {
            ServerMessage::Bolid { context, event } => {
                if Some(context) != self.application.game.session_context() {
                    return;
                }
                self.bolids.retain(|(existing, bolid, at, _)| {
                    *existing == context
                        && *at + Duration::from_secs(150) > Instant::now()
                        && bolid.id != event.id
                });
                if self.bolids.len() == minimium_renderer::milky_way_gpu::MAX_BOLIDS {
                    self.bolids.remove(0);
                }
                self.bolids.push((context, event, Instant::now(), None));
            }
            message @ ServerMessage::Welcome { .. } => self.handle_welcome(message),
            ServerMessage::Pong if self.application.ui.console.open => {
                self.application.ui.console.push_client_text(
                    Text::Pong,
                    false,
                    &self.localizer,
                    self.application.settings.locale,
                );
            }
            ServerMessage::AltitudeResult {
                context,
                position_mm,
            } => {
                self.handle_altitude_result(context, position_mm);
            }
            ServerMessage::MapTeleportResult { position_mm } => {
                self.handle_map_teleport_result(position_mm);
            }
            ServerMessage::Accepted if self.application.ui.console.open => {
                self.application.ui.console.push_client_text(
                    Text::Accepted,
                    false,
                    &self.localizer,
                    self.application.settings.locale,
                );
            }
            ServerMessage::Error(code) => {
                self.application.ui.console.push_client_text(
                    Text::ServerError { code },
                    true,
                    &self.localizer,
                    self.application.settings.locale,
                );
            }
            ServerMessage::ShotResult {
                target_id,
                remaining_health,
                killed,
            } => {
                self.handle_shot_result(target_id, remaining_health, killed);
            }
            ServerMessage::HealthUpdate(health) => self.handle_health_update(health),
            ServerMessage::Chat {
                sender_id,
                sender_name,
                text,
            } => self.application.ui.console.push_chat_line(
                &Text::Chat {
                    sender_name,
                    sender_id,
                    text,
                }
                .localized(&self.localizer, self.application.settings.locale),
            ),
            ServerMessage::Bye => {
                if self.application.ui.console.open {
                    self.application.ui.console.push_client_text(
                        Text::ServerClosed,
                        false,
                        &self.localizer,
                        self.application.settings.locale,
                    );
                }
                self.clear_connection();
            }
            message @ ServerMessage::Snapshot { .. } => self.handle_snapshot(message),
            ServerMessage::TransitionPrepare {
                transition_id,
                source_context,
                target_context,
                map_fingerprint,
                destination,
                manifest,
                ..
            } => self.handle_transition_prepare(
                transition_id,
                source_context,
                target_context,
                map_fingerprint,
                destination,
                manifest,
            ),
            ServerMessage::TransitionCommitted {
                transition_id,
                context,
                map_fingerprint,
                destination,
                destination_mm,
                health,
                manifest,
                ..
            } => self.handle_transition_committed(
                transition_id,
                context,
                map_fingerprint,
                destination,
                destination_mm,
                health,
                manifest,
            ),
            ServerMessage::TransitionCancelled {
                transition_id,
                context,
            } => {
                self.handle_transition_cancelled(transition_id, context);
            }
            _ => {}
        }
    }
}
