use super::*;

mod connection;
mod gameplay;
mod transitions;

impl DesktopHost {
    pub(super) fn handle_server_message(
        &mut self,
        message: ServerMessage,
        update: Result<
            Option<crate::application::WorldContextChange>,
            crate::application::ApplicationMessageError,
        >,
    ) {
        match message {
            ServerMessage::EnvironmentSync { clock_sample, .. }
                if clock_sample.rate_ppm().get() == 0 =>
            {
                self.application.input.clear_held();
                self.input.jump_pending = false;
            }
            ServerMessage::Excavation { context, message } => {
                self.observe_excavation(context, &message);
            }
            ServerMessage::ImpactState {
                context,
                body,
                live,
                server_time_ms,
            } => {
                if Some(context) == self.application.core_view().context {
                    self.impacts
                        .observe(context, body, live, server_time_ms as f64 / 1000.0);
                }
            }
            ServerMessage::Bolid { context, event } => {
                if Some(context) != self.application.core_view().context {
                    return;
                }
                self.bolids
                    .retain(|(existing, bolid, _)| *existing == context && bolid.id != event.id);
                if self.bolids.len() == minimium_renderer::milky_way_gpu::MAX_BOLIDS {
                    self.bolids.remove(0);
                }
                self.bolids.push((context, event, None));
            }
            message @ ServerMessage::Welcome { .. } => self.handle_welcome(message, update),
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
                self.handle_altitude_result(context, position_mm, update);
            }
            ServerMessage::MapTeleportResult { position_mm } => {
                self.handle_map_teleport_result(position_mm, update);
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
            ServerMessage::HealthUpdate(health) => self.handle_health_update(health, update),
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
            ServerMessage::Snapshot { .. } => self.handle_snapshot(update),
            ServerMessage::TransitionPrepare {
                transition_id,
                source_context,
                target_context,
                map_fingerprint,
                destination,
                manifest,
                ..
            } => {
                self.handle_transition_prepare(
                    transition_id,
                    source_context,
                    target_context,
                    map_fingerprint,
                    destination,
                    manifest,
                );
            }
            ServerMessage::TransitionCommitted {
                transition_id,
                context,
                ..
            } => self.handle_transition_committed(transition_id, context, update),
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
