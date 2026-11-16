use super::{events, help_lines, properties, start_error_output, start_remote, ClientOptions};
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
        FrontendAction::Help => {
            for line in help_lines() {
                println!("{line}");
            }
            println!(
                "{}",
                Text::FpsUnavailable.localized(&localizer, options.locale)
            );
        }
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
        FrontendAction::Clear => {
            result.outputs.push(CommandOutput::Clear);
        }
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
        FrontendAction::Property(command) => properties::print_result(
            text_properties.execute(command, options, application.connection_view(), &localizer),
            &localizer,
            options.locale,
        ),
        FrontendAction::ConfigSave(path) => {
            match crate::config::save_preferences(
                &path,
                &options.config_file_settings,
                &text_properties.session_settings(),
            ) {
                Ok(()) => println!(
                    "{}",
                    localizer.format(
                        options.locale,
                        &minimium_localization::UiMessage::ConfigurationSaved(
                            path.display().to_string()
                        )
                    )
                ),
                Err(error) => {
                    application.record_diagnostic_failure(
                        "storage.config_save_failed",
                        &error.to_string(),
                    );
                    println!(
                        "{}",
                        localizer.format(
                            options.locale,
                            &minimium_localization::UiMessage::ConfigurationSaveFailed
                        )
                    );
                }
            }
        }
        FrontendAction::SetTime { minute, local } => {
            let longitude = match last_snapshot.as_ref() {
                Some(ServerMessage::Snapshot {
                    self_position_mm, ..
                }) if local => {
                    let [x, _, z] = self_position_mm.0.map(|value| value as f64);
                    (x.hypot(z) > 0.0).then(|| z.atan2(x))
                }
                _ if !local => Some(0.0),
                _ => None,
            };
            let result = longitude
                .ok_or_else(|| "player longitude is unavailable".to_owned())
                .and_then(|longitude| {
                    application
                        .request_set_time(minute, longitude, crate::host_time::sample())
                        .map_err(|error| error.to_string())
                });
            match result {
                Ok(()) => println!(
                    "{}",
                    localizer.format(
                        options.locale,
                        &minimium_localization::UiMessage::Notice(
                            minimium_localization::UiNotice::TimeRequestSent
                        )
                    )
                ),
                Err(error) => {
                    application.record_diagnostic_failure("client.time_request_failed", &error);
                    println!(
                        "{}",
                        localizer.format(
                            options.locale,
                            &minimium_localization::UiMessage::Notice(
                                minimium_localization::UiNotice::TimeRequestFailed
                            )
                        )
                    );
                }
            }
        }
        FrontendAction::TimeStatus => {
            let unavailable = crate::environment::EnvironmentFrameInput::unavailable();
            let status = application
                .session()
                .map(|session| session.environment_frame().time_status_line())
                .unwrap_or_else(|| unavailable.time_status_line());
            println!("{status}");
        }
        FrontendAction::TimeScale(value) => match application
            .request_time_scale(value, crate::host_time::sample())
        {
            Ok(()) => println!(
                "{}",
                localizer.format(
                    options.locale,
                    &minimium_localization::UiMessage::Notice(
                        minimium_localization::UiNotice::TimeScaleRequestSent
                    )
                )
            ),
            Err(error) => {
                application.record_diagnostic_failure("client.time_scale_request_failed", &error);
                println!(
                    "{}",
                    localizer.format(
                        options.locale,
                        &minimium_localization::UiMessage::Notice(
                            minimium_localization::UiNotice::TimeScaleRequestFailed
                        )
                    )
                );
            }
        },
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
        FrontendAction::TimeMode(mode) => match application
            .request_time_mode(&mode, crate::host_time::sample())
        {
            Ok(()) => println!(
                "{}",
                localizer.format(
                    options.locale,
                    &minimium_localization::UiMessage::Notice(
                        minimium_localization::UiNotice::TimeModeRequestSent
                    )
                )
            ),
            Err(error) => {
                application.record_diagnostic_failure("client.time_mode_request_failed", &error);
                println!(
                    "{}",
                    localizer.format(
                        options.locale,
                        &minimium_localization::UiMessage::Notice(
                            minimium_localization::UiNotice::TimeModeRequestFailed
                        )
                    )
                );
            }
        },
        FrontendAction::TimeOfflinePolicy(policy) => {
            match application.request_time_offline_policy(&policy, crate::host_time::sample()) {
                Ok(()) => println!(
                    "{}",
                    localizer.format(
                        options.locale,
                        &minimium_localization::UiMessage::Notice(
                            minimium_localization::UiNotice::TimeOfflinePolicyRequestSent
                        )
                    )
                ),
                Err(error) => {
                    application
                        .record_diagnostic_failure("client.time_policy_request_failed", &error);
                    println!(
                        "{}",
                        localizer.format(
                            options.locale,
                            &minimium_localization::UiMessage::Notice(
                                minimium_localization::UiNotice::TimeOfflinePolicyRequestFailed
                            )
                        )
                    );
                }
            }
        }
        FrontendAction::Presentation(operation) => {
            let capability = match operation {
                crate::client_commands::PresentationOperation::Developer(_) => {
                    crate::client_commands::PresentationCapability::DeveloperRenderer
                }
                crate::client_commands::PresentationOperation::AtmosphereStatus
                | crate::client_commands::PresentationOperation::WeatherStatus
                | crate::client_commands::PresentationOperation::TerrainStatus => {
                    crate::client_commands::PresentationCapability::WeatherDiagnostics
                }
                crate::client_commands::PresentationOperation::SkyInfo
                | crate::client_commands::PresentationOperation::SkyBodies => {
                    crate::client_commands::PresentationCapability::SkyDiagnostics
                }
                crate::client_commands::PresentationOperation::MeteorStatus
                | crate::client_commands::PresentationOperation::MeteorSpawn(_, _)
                | crate::client_commands::PresentationOperation::MeteorClear
                | crate::client_commands::PresentationOperation::MeteorLight(_)
                | crate::client_commands::PresentationOperation::MeteorDebug(_) => {
                    crate::client_commands::PresentationCapability::MeteorDiagnostics
                }
            };
            unavailable(&mut result, capability);
        }
    }
    (false, result)
}

fn unavailable(
    result: &mut CommandActionResult,
    capability: crate::client_commands::PresentationCapability,
) {
    result.outputs.push(CommandOutput::Presentation(
        crate::client_commands::PresentationResult::Unavailable(capability),
    ));
}
