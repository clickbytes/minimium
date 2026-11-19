use super::output::CommandOutputCollector;
use super::*;
use crate::messages::ClientText as Text;

pub(super) fn execute(
    renderer: &mut DesktopHost,
    action: FrontendAction,
) -> (bool, client_commands::CommandActionResult) {
    let mut output = CommandOutputCollector::default();
    match action {
        FrontendAction::Notice(notice) => {
            output.push_ui_message(minimium_localization::UiMessage::Notice(notice))
        }
        FrontendAction::Print(text) => {
            for line in text.lines() {
                output.push_line(line);
            }
        }
        FrontendAction::Help | FrontendAction::Clear => unreachable!("shared console operation"),
        FrontendAction::Connect(address) => {
            if renderer.application.connection_view().busy() {
                output.push_client_text(Text::DisconnectOrWait, true);
            } else {
                output
                    .result
                    .outputs
                    .extend(renderer.start_remote_attempt(&address).outputs);
            }
        }
        FrontendAction::HostLocal(lane) => {
            if renderer.application.connection_view().busy() {
                output.push_client_text(Text::DisconnectOrWait, true);
            } else {
                output
                    .result
                    .outputs
                    .extend(renderer.start_local_attempt(lane).outputs);
            }
        }
        FrontendAction::Disconnect => {
            if let Err(error) = renderer
                .application
                .cancel_connection_attempt(crate::host_time::sample())
            {
                renderer
                    .application
                    .record_diagnostic_failure("client.invalid_time", &error);
                return (false, output.result);
            }
            renderer.clear_connection();
            output.push_client_text(Text::Disconnected, false);
        }
        FrontendAction::FpsShow => output.push_line(renderer.metrics.frame_stats.summary()),
        FrontendAction::NetStats => {
            let metrics = renderer
                .application
                .connection_view()
                .session()
                .map(ClientSession::metrics)
                .unwrap_or_default();
            output.push_client_text(
                Text::NetworkStats {
                    bytes_sent: renderer
                        .metrics
                        .previous_bytes_sent
                        .saturating_add(metrics.bytes_sent),
                    bytes_received: renderer
                        .metrics
                        .previous_bytes_received
                        .saturating_add(metrics.bytes_received),
                    ping_milliseconds: metrics.ping_round_trip.map(|duration| duration.as_millis()),
                    nearby_players: Some(renderer.application.core_view().remote_player_count),
                },
                false,
            );
        }
        FrontendAction::State => output.push_client_text(
            Text::PlayerState {
                position: renderer.application.core_view().character.position,
                health: renderer.application.core_view().health,
                nearby_players: renderer.application.core_view().remote_player_count,
            },
            false,
        ),
        FrontendAction::MapTeleport => {
            if let Some(unavailable) = super::presentation::unavailable_for(
                renderer,
                client_commands::PresentationCapability::EarthMapTeleport,
                client_commands::PresentationRequirement::ActiveEarthWorld,
            ) {
                output
                    .result
                    .outputs
                    .push(client_commands::CommandOutput::Presentation(
                        client_commands::PresentationResult::Unavailable(unavailable),
                    ));
            } else {
                renderer.arm_map_teleport();
            }
        }
        FrontendAction::Property(command) => {
            for line in renderer.apply_property(command).lines() {
                output.push_line(line);
            }
        }
        FrontendAction::CameraStatus
        | FrontendAction::CameraLook { .. }
        | FrontendAction::ConfigSave(_) => unreachable!("shared preference operation"),
        FrontendAction::Jump | FrontendAction::Movement(..) => {
            unreachable!("shared policy operations")
        }
        FrontendAction::Clock(_) => unreachable!("clock actions use the shared time adapter"),
        FrontendAction::SpawnRandomBolid(count) => {
            if let Some(unavailable) = super::presentation::unavailable_for(
                renderer,
                client_commands::PresentationCapability::DeveloperRenderer,
                client_commands::PresentationRequirement::ActiveWorldPresentation,
            ) {
                output
                    .result
                    .outputs
                    .push(client_commands::CommandOutput::Presentation(
                        client_commands::PresentationResult::Unavailable(unavailable),
                    ));
            } else {
                // Publish the current gaze before the separate admin request,
                // including when the camera just moved.
                renderer.pending_bolid_request = Some(count);
                renderer.last_sky_view_at = None;
                output.push_ui_message(minimium_localization::UiMessage::Notice(
                    minimium_localization::UiNotice::BolidRequestQueued,
                ));
            }
        }
        FrontendAction::SpawnRandomMeteorid => {
            let text = match renderer
                .application
                .request_random_meteorid(crate::host_time::sample())
            {
                Ok(()) => "Meteorite request sent".to_owned(),
                Err(error) => format!("spawn_random_meteorid: {error}"),
            };
            output.push_line(text);
        }
        FrontendAction::Presentation(operation) => {
            let result = super::presentation::execute(renderer, operation);
            output
                .result
                .outputs
                .push(client_commands::CommandOutput::Presentation(result));
        }
        FrontendAction::Quit => return (true, output.result),
    }
    (false, output.result)
}
