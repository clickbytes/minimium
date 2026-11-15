use super::output::CommandOutputCollector;
use super::*;

pub(super) fn execute(
    renderer: &mut Renderer,
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
        FrontendAction::Help => {
            output.push_ui_message(minimium_localization::UiMessage::Notice(
                minimium_localization::UiNotice::CommandsHeader,
            ));
            for line in client_commands::help_lines() {
                output.push_line(line);
            }
            output.push_client_text(Text::ConsoleHistoryHelp, false);
        }
        FrontendAction::Connect(address) => {
            if renderer.application.connection.busy() {
                output.push_client_text(Text::DisconnectOrWait, true);
            } else {
                output
                    .result
                    .outputs
                    .extend(renderer.start_remote_attempt(&address).outputs);
            }
        }
        FrontendAction::HostLocal(lane) => {
            if renderer.application.connection.busy() {
                output.push_client_text(Text::DisconnectOrWait, true);
            } else {
                output
                    .result
                    .outputs
                    .extend(renderer.start_local_attempt(lane).outputs);
            }
        }
        FrontendAction::Disconnect => {
            renderer.application.connection.cancel();
            renderer.clear_connection();
            output.push_client_text(Text::Disconnected, false);
        }
        FrontendAction::FpsShow => renderer
            .application
            .ui
            .console
            .push_line(renderer.metrics.frame_stats.summary()),
        FrontendAction::NetStats => {
            let metrics = renderer
                .application
                .connection
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
                    nearby_players: Some(
                        renderer
                            .application
                            .game
                            .gameplay
                            .reconciliation
                            .player_count(),
                    ),
                },
                false,
            );
        }
        FrontendAction::State => output.push_client_text(
            Text::PlayerState {
                position: renderer
                    .application
                    .game
                    .gameplay
                    .prediction
                    .character
                    .position,
                health: renderer.application.game.gameplay.health,
                nearby_players: renderer
                    .application
                    .game
                    .gameplay
                    .reconciliation
                    .player_count(),
            },
            false,
        ),
        FrontendAction::MapTeleport => renderer.arm_map_teleport(),
        FrontendAction::Clear => output.clear_lines(),
        FrontendAction::Property(command) => {
            for line in renderer.apply_property(command).lines() {
                output.push_line(line);
            }
        }
        FrontendAction::ConfigSave(path) => {
            match crate::config::save_preferences(
                &path,
                &renderer.application.settings.config_file_settings,
                &renderer.session_settings(),
            ) {
                Ok(()) => {
                    output.push_ui_message(minimium_localization::UiMessage::ConfigurationSaved(
                        path.display().to_string(),
                    ))
                }
                Err(error) => {
                    renderer.application.connection.record_diagnostic_failure(
                        "storage.config_save_failed",
                        &error.to_string(),
                    );
                    output.push_ui_error_message(
                        minimium_localization::UiMessage::ConfigurationSaveFailed,
                    );
                }
            }
        }
        FrontendAction::SetTime { minute, local } => {
            let [x, _, z] = renderer.application.game.view().player_position_m;
            let result = if local && (!renderer.is_earth_world() || x.hypot(z) < 1.0e-6) {
                Err(minimium_client_runtime::ControllerError::new(
                    "client.local_time_unavailable",
                    "local solar time requires an Earth observer away from the poles",
                ))
            } else {
                renderer
                    .application
                    .connection
                    .request_set_time(minute, if local { z.atan2(x) } else { 0.0 })
            };
            match result {
                Ok(()) => renderer
                    .application
                    .ui
                    .console
                    .push_line(renderer.localizer.format(
                        renderer.application.settings.locale,
                        &minimium_localization::UiMessage::Notice(
                            minimium_localization::UiNotice::TimeRequestSent,
                        ),
                    )),
                Err(error) => {
                    renderer
                        .application
                        .connection
                        .record_diagnostic_failure("client.time_request_failed", &error);
                    output.push_ui_error_message(minimium_localization::UiMessage::Notice(
                        minimium_localization::UiNotice::TimeRequestFailed,
                    ));
                }
            }
        }
        FrontendAction::TimeStatus => renderer
            .application
            .ui
            .console
            .push_line(renderer.environment_frame.time_status_line()),
        FrontendAction::TimeScale(value) => {
            match renderer.application.connection.request_time_scale(value) {
                Ok(()) => renderer
                    .application
                    .ui
                    .console
                    .push_line(renderer.localizer.format(
                        renderer.application.settings.locale,
                        &minimium_localization::UiMessage::Notice(
                            minimium_localization::UiNotice::TimeScaleRequestSent,
                        ),
                    )),
                Err(error) => {
                    renderer
                        .application
                        .connection
                        .record_diagnostic_failure("client.time_scale_request_failed", &error);
                    output.push_ui_error_message(minimium_localization::UiMessage::Notice(
                        minimium_localization::UiNotice::TimeScaleRequestFailed,
                    ));
                }
            }
        }
        FrontendAction::SpawnRandomBolid(count) => {
            // Publish the current gaze before the separate admin request, also
            // when already connected and the camera just moved.
            renderer.pending_bolid_request = Some(count);
            renderer.last_sky_view_at = None;
            renderer.bolid_notice(minimium_localization::UiNotice::BolidRequestQueued);
        }
        FrontendAction::SpawnRandomMeteorid => {
            let text = match renderer.application.connection.request_random_meteorid() {
                Ok(()) => "Meteorite request sent".to_owned(),
                Err(error) => format!("spawn_random_meteorid: {error}"),
            };
            output.push_line(text);
        }
        FrontendAction::TimeMode(mode) => {
            match renderer.application.connection.request_time_mode(&mode) {
                Ok(()) => renderer
                    .application
                    .ui
                    .console
                    .push_line(renderer.localizer.format(
                        renderer.application.settings.locale,
                        &minimium_localization::UiMessage::Notice(
                            minimium_localization::UiNotice::TimeModeRequestSent,
                        ),
                    )),
                Err(error) => {
                    renderer
                        .application
                        .connection
                        .record_diagnostic_failure("client.time_mode_request_failed", &error);
                    output.push_ui_error_message(minimium_localization::UiMessage::Notice(
                        minimium_localization::UiNotice::TimeModeRequestFailed,
                    ));
                }
            }
        }
        FrontendAction::TimeOfflinePolicy(policy) => {
            match renderer
                .application
                .connection
                .request_time_offline_policy(&policy)
            {
                Ok(()) => renderer
                    .application
                    .ui
                    .console
                    .push_line(renderer.localizer.format(
                        renderer.application.settings.locale,
                        &minimium_localization::UiMessage::Notice(
                            minimium_localization::UiNotice::TimeOfflinePolicyRequestSent,
                        ),
                    )),
                Err(error) => {
                    renderer
                        .application
                        .connection
                        .record_diagnostic_failure("client.time_policy_request_failed", &error);
                    output.push_ui_error_message(minimium_localization::UiMessage::Notice(
                        minimium_localization::UiNotice::TimeOfflinePolicyRequestFailed,
                    ));
                }
            }
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
