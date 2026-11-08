use super::{events, help_lines, print_start_error, properties, start_remote, ClientOptions};
use crate::client_commands::FrontendAction;
use crate::messages::ClientText as Text;
use crate::session::SessionEvent;
use minimium_client_runtime::ConnectionController;
use minimium_localization::Localizer;
use minimium_protocol::ServerMessage;
use std::{
    io::{self, Write},
    path::Path,
};

pub(super) fn handle(
    action: FrontendAction,
    options: &mut ClientOptions,
    connection: &mut ConnectionController,
    last_snapshot: &mut Option<ServerMessage>,
    localizer: &Localizer,
    data_dir: &Path,
    log_dir: &Path,
    parent_run_id: &str,
    text_properties: &mut properties::TextProperties,
) -> io::Result<bool> {
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
            if connection.busy() {
                println!(
                    "{}",
                    Text::DisconnectOrWait.localized(&localizer, options.locale)
                );
                return Ok(false);
            }
            let config = match options.local_server_launch_config(
                data_dir.to_path_buf(),
                log_dir.to_path_buf(),
                parent_run_id.to_owned(),
                requested_lanes,
            ) {
                Ok(config) => config,
                Err(error) => {
                    connection.record_diagnostic_failure(
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
                    return Ok(false);
                }
            };
            match connection.start_local(config) {
                Ok(()) => {
                    println!(
                        "{}",
                        Text::StartingLocalServer.localized(&localizer, options.locale)
                    );
                }
                Err(error) => print_start_error(error, &localizer, options.locale),
            }
        }
        FrontendAction::Connect(address) => {
            if connection.busy() {
                println!(
                    "{}",
                    Text::DisconnectOrWait.localized(&localizer, options.locale)
                );
            } else {
                start_remote(connection, &address, &localizer, options.locale);
            }
        }
        FrontendAction::Disconnect => {
            connection.cancel();
            if let Some(mut current) = connection.take_session() {
                current.close();
            }
            *last_snapshot = None;
            println!(
                "{}",
                Text::Disconnected.localized(&localizer, options.locale)
            );
        }
        FrontendAction::State => {
            if connection.session().is_none() {
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
        FrontendAction::NetStats => match connection.session() {
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
            print!("\x1B[2J\x1B[H");
            io::stdout().flush()?;
        }
        FrontendAction::Quit => return Ok(true),
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
        FrontendAction::SkyInfo => {
            println!(
                "{}",
                localizer.format(
                    options.locale,
                    &minimium_localization::UiMessage::Notice(
                        minimium_localization::UiNotice::SkyInfoDesktopOnly
                    )
                )
            );
        }
        FrontendAction::SkyBodies => {
            println!(
                "{}",
                localizer.format(
                    options.locale,
                    &minimium_localization::UiMessage::Notice(
                        minimium_localization::UiNotice::SkyInfoDesktopOnly
                    )
                )
            );
        }
        FrontendAction::Property(command) => properties::print_result(
            text_properties.execute(command, options, &connection, &localizer),
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
                    connection.record_diagnostic_failure(
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
        FrontendAction::WeatherStatus
        | FrontendAction::AtmosphereStatus
        | FrontendAction::TerrainStatus => {
            println!(
                "{}",
                localizer.format(
                    options.locale,
                    &minimium_localization::UiMessage::Notice(
                        minimium_localization::UiNotice::WeatherRequiresDesktop
                    )
                )
            );
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
                    connection
                        .request_set_time(minute, longitude)
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
                    connection.record_diagnostic_failure("client.time_request_failed", &error);
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
            let status = connection
                .session()
                .map(|session| session.environment_frame().time_status_line())
                .unwrap_or_else(|| unavailable.time_status_line());
            println!("{status}");
        }
        FrontendAction::TimeScale(value) => match connection.request_time_scale(value) {
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
                connection.record_diagnostic_failure("client.time_scale_request_failed", &error);
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
        FrontendAction::SpawnRandomBolid => match connection.request_random_bolid() {
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
        },
        FrontendAction::TimeMode(mode) => match connection.request_time_mode(&mode) {
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
                connection.record_diagnostic_failure("client.time_mode_request_failed", &error);
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
            match connection.request_time_offline_policy(&policy) {
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
                    connection
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
    }
    Ok(false)
}
