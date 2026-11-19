use super::output::CommandOutputCollector;
use super::*;

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
        FrontendAction::Connect(_)
        | FrontendAction::HostLocal(_)
        | FrontendAction::Disconnect
        | FrontendAction::NetStats
        | FrontendAction::State
        | FrontendAction::Quit
        | FrontendAction::Ping => unreachable!("shared connection binding"),
        FrontendAction::FpsShow => output.push_line(renderer.metrics.frame_stats.summary()),
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
    }
    (false, output.result)
}
