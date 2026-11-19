use super::{events, properties, start_error_output, start_remote, ClientOptions};
use crate::application::ClientApplication;
use crate::client_commands::{CommandActionResult, CommandOutput, FrontendAction};
use crate::messages::ClientText as Text;
use crate::session::SessionEvent;
use minimium_localization::Localizer;
use minimium_protocol::ServerMessage;
use std::path::Path;

pub(super) fn handle(
    action: FrontendAction,
    options: &mut ClientOptions,
    application: &mut ClientApplication,
    last_snapshot: &mut Option<ServerMessage>,
    localizer: &Localizer,
    data_dir: &Path,
    log_dir: &Path,
    parent_run_id: &str,
    text_properties: &mut properties::TextProperties,
) -> (bool, CommandActionResult) {
    let mut result = CommandActionResult::default();
    macro_rules! println {
        ($($argument:tt)*) => {
            {
                result.outputs.push(CommandOutput::Line(format!($($argument)*)));
            }
        };
    }
    match action {
        FrontendAction::Notice(notice) => println!(
            "{}",
            localizer.format(
                options.locale,
                &minimium_localization::UiMessage::Notice(notice),
            )
        ),
        FrontendAction::Print(text) => println!("{text}"),
        FrontendAction::Help | FrontendAction::Clear => unreachable!("shared console operation"),
        FrontendAction::HostLocal(requested_lanes) => {
            if application.is_connection_busy() {
                println!(
                    "{}",
                    Text::DisconnectOrWait.localized(&localizer, options.locale)
                );
                return (false, result);
            }
            let config = match options.local_server_launch_config(
                data_dir.to_path_buf(),
                log_dir.to_path_buf(),
                parent_run_id.to_owned(),
                requested_lanes,
            ) {
                Ok(config) => config,
                Err(error) => {
                    application.record_diagnostic_failure(
                        "server.local_launch_config_failed",
                        &error.to_string(),
                    );
                    println!(
                        "{}",
                        localizer.format(
                            options.locale,
                            &minimium_localization::UiMessage::Notice(
                                minimium_localization::UiNotice::LocalServerConfigFailed,
                            ),
                        )
                    );
                    return (false, result);
                }
            };
            match application.start_local(config, crate::host_time::sample()) {
                Ok(()) => {
                    println!(
                        "{}",
                        Text::StartingLocalServer.localized(&localizer, options.locale)
                    );
                }
                Err(error) => result.outputs.extend(start_error_output(error).outputs),
            }
        }
        FrontendAction::Connect(address) => {
            if application.is_connection_busy() {
                println!(
                    "{}",
                    Text::DisconnectOrWait.localized(&localizer, options.locale)
                );
            } else {
                result
                    .outputs
                    .extend(start_remote(application, &address).outputs);
            }
        }
        FrontendAction::Disconnect => {
            if let Err(error) = application.cancel_connection_attempt(crate::host_time::sample()) {
                println!("{error}");
                return (false, result);
            }
            match application.disconnect_session(crate::host_time::sample()) {
                Ok(Some(mut current)) => {
                    if let Err(error) = current.close(crate::host_time::sample()) {
                        println!("{error}");
                    }
                }
                Ok(None) => {}
                Err(error) => {
                    println!("{error}");
                    return (false, result);
                }
            }
            *last_snapshot = None;
            println!(
                "{}",
                Text::Disconnected.localized(&localizer, options.locale)
            );
        }
        FrontendAction::State => {
            if application.session().is_none() {
                println!(
                    "{}",
                    Text::NotConnected.localized(&localizer, options.locale)
                );
            } else if let Some(snapshot) = last_snapshot.as_ref() {
                events::print_session_event(
                    SessionEvent::Message(snapshot.clone()),
                    true,
                    &localizer,
                    options.locale,
                );
            } else {
                println!(
                    "{}",
                    Text::NoServerResponse.localized(&localizer, options.locale)
                );
            }
        }
        FrontendAction::FpsShow => {
            println!(
                "{}",
                Text::FpsUnavailable.localized(&localizer, options.locale)
            );
        }
        FrontendAction::NetStats => match application.session() {
            Some(current) => {
                let metrics = current.metrics();
                println!(
                    "{}",
                    Text::NetworkStats {
                        bytes_sent: metrics.bytes_sent,
                        bytes_received: metrics.bytes_received,
                        ping_milliseconds: metrics
                            .ping_round_trip
                            .map(|duration| duration.as_millis()),
                        nearby_players: None,
                    }
                    .localized(&localizer, options.locale)
                );
            }
            None => println!(
                "{}",
                Text::NotConnected.localized(&localizer, options.locale)
            ),
        },
        FrontendAction::Quit => return (true, result),
        FrontendAction::MapTeleport => {
            println!(
                "{}",
                localizer.format(
                    options.locale,
                    &minimium_localization::UiMessage::Notice(
                        minimium_localization::UiNotice::MapTeleportDesktopOnly
                    )
                )
            );
        }
        FrontendAction::Property(command) => {
            match text_properties.execute(command, options, application.connection_view()) {
                Ok(line) => result.outputs.push(CommandOutput::Line(line)),
                Err(error) => {
                    result
                        .outputs
                        .push(CommandOutput::Line(crate::client_commands::command_error(
                            &error,
                            localizer,
                            options.locale,
                        )))
                }
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
            match application.request_random_bolid(count, crate::host_time::sample()) {
                Ok(()) => println!(
                    "{}",
                    localizer.format(
                        options.locale,
                        &minimium_localization::UiMessage::Notice(
                            minimium_localization::UiNotice::BolidRequestSent
                        )
                    )
                ),
                Err(error) => println!("spawn_random_bolid: {error}"),
            }
        }
        FrontendAction::SpawnRandomMeteorid => {
            match application.request_random_meteorid(crate::host_time::sample()) {
                Ok(()) => println!("Meteorite request sent"),
                Err(error) => println!("spawn_random_meteorid: {error}"),
            }
        }
        FrontendAction::Presentation(operation) => {
            let unavailable_result = crate::client_commands::PresentationUnavailable {
                capability: operation.capability(),
                requirement: operation.requirement(),
                reason: crate::client_commands::PresentationUnavailableReason::UnsupportedHost,
            };
            result.outputs.push(CommandOutput::Presentation(
                crate::client_commands::PresentationResult::Unavailable(unavailable_result),
            ));
        }
    }
    (false, result)
}
