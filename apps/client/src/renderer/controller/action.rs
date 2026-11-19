use super::output::CommandOutputCollector;
use super::*;

pub(super) fn execute(
    renderer: &mut DesktopHost,
    action: FrontendAction,
) -> (bool, client_commands::CommandActionResult) {
    let mut output = CommandOutputCollector::default();
    match action {
        FrontendAction::Print(text) => {
            for line in text.lines() {
                output.push_line(line);
            }
        }
        FrontendAction::Player(_) => unreachable!("shared player binding"),
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
            unreachable!("map selection uses the typed controller result")
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
            // A current gaze is useful input; the server still selects and validates
            // the observer. Rendering readiness never grants admin permission.
            renderer.pending_bolid_request = Some(count);
            renderer.last_sky_view_at = None;
            output.push_ui_message(minimium_localization::UiMessage::Notice(
                minimium_localization::UiNotice::BolidRequestQueued,
            ));
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
        FrontendAction::BolidStatus(id) => {
            let text = match renderer
                .application
                .request_bolid_shower_status(id, crate::host_time::sample())
            {
                Ok(()) => format!("bolid shower {id} status requested"),
                Err(error) => format!("bolids status: {error}"),
            };
            output.push_line(text);
        }
        FrontendAction::SaveWorld => {
            let text = match renderer
                .application
                .request_persistence_save(crate::host_time::sample())
            {
                Ok(()) => "save requested; await owner job receipt".to_owned(),
                Err(error) => format!("world save: {error}"),
            };
            output.push_line(text);
        }
        FrontendAction::SaveStatus(id) => {
            let text = match renderer
                .application
                .request_persistence_status(id, crate::host_time::sample())
            {
                Ok(()) => "save status requested".to_owned(),
                Err(error) => format!("server.persistence.status: {error}"),
            };
            output.push_line(text);
        }
        FrontendAction::ImpactStatus { world, id } => {
            let text = match renderer.application.request_impact_receipt_status(
                world,
                id,
                crate::host_time::sample(),
            ) {
                Ok(()) => format!("impact {world}/{id} status requested"),
                Err(error) => format!("impacts status: {error}"),
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
