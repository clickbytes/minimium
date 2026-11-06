use super::*;

mod connection;
mod gameplay;
mod transitions;

impl Renderer {
    pub(super) fn handle_server_message(&mut self, message: ServerMessage) {
        match message {
            message @ ServerMessage::Welcome { .. } => self.handle_welcome(message),
            ServerMessage::Pong if self.ui.console.open => {
                self.ui.console.push_line(Text::Pong.to_string());
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
            ServerMessage::Accepted if self.ui.console.open => {
                self.ui.console.push_line(Text::Accepted.to_string());
            }
            ServerMessage::Error(code) => {
                self.ui
                    .console
                    .push_error(Text::ServerError { code }.to_string());
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
            } => self.ui.console.push_chat_line(
                &Text::Chat {
                    sender_name,
                    sender_id,
                    text,
                }
                .to_string(),
            ),
            ServerMessage::Bye => {
                if self.ui.console.open {
                    self.ui.console.push_line(Text::ServerClosed.to_string());
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
                context,
                map_fingerprint,
                destination,
                destination_mm,
                health,
                manifest,
                ..
            } => self.handle_transition_committed(
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
