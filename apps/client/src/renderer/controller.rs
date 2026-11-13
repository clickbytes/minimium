use super::*;
use crate::messages::ClientText as Text;

pub(super) fn execute_console_input(renderer: &mut Renderer) -> bool {
    let input = renderer
        .application
        .ui
        .console
        .take_input(&renderer.localizer, renderer.application.settings.locale);
    let command = match client_commands::parse_command_typed(&input) {
        Ok(Some(command)) => command,
        Ok(None) => return false,
        Err(error) => {
            renderer.application.ui.console.push_ui_error_message(
                minimium_localization::UiMessage::CommandRejected {
                    code: error.code.as_str().to_owned(),
                    subject: error.subject.clone().unwrap_or_default(),
                },
                &renderer.localizer,
                renderer.application.settings.locale,
            );
            if let Some(usage) = error.usage.as_deref() {
                renderer
                    .application
                    .ui
                    .console
                    .push_line(format!("Usage: {usage}"));
            }
            return false;
        }
    };
    let direction = super::camera::camera_direction(
        renderer.application.game.gameplay.prediction.character,
        renderer.input.yaw,
        renderer.input.pitch,
    )
    .normalize()
    .to_array();
    let player_position = renderer
        .application
        .game
        .gameplay
        .prediction
        .character
        .position;
    let result = client_commands::execute(
        command,
        &mut CommandContext {
            connection: &mut renderer.application.connection,
            fire_direction: direction,
            player_position: Some(player_position),
        },
    );
    let action = match result {
        CommandResult::Sent => {
            renderer.application.ui.console.push_client_text(
                Text::CommandQueued,
                false,
                &renderer.localizer,
                renderer.application.settings.locale,
            );
            return false;
        }
        CommandResult::QueueError(error) => {
            renderer.application.ui.console.push_client_text(
                Text::CouldNotQueueCommand(error.into()),
                true,
                &renderer.localizer,
                renderer.application.settings.locale,
            );
            return false;
        }
        CommandResult::Action(action) => action,
    };
    match action {
        FrontendAction::Notice(notice) => renderer.application.ui.console.push_ui_message(
            minimium_localization::UiMessage::Notice(notice),
            &renderer.localizer,
            renderer.application.settings.locale,
        ),
        FrontendAction::AtmosphereStatus => {
            for line in renderer.gpu.atmosphere_gpu.status() {
                renderer.application.ui.console.push_line(line);
            }
        }
        FrontendAction::WeatherStatus => {
            for line in renderer.weather_status() {
                renderer.application.ui.console.push_line(line);
            }
        }
        FrontendAction::TerrainStatus => {
            for line in renderer.terrain_status() {
                renderer.application.ui.console.push_line(line);
            }
        }
        FrontendAction::Print(text) => {
            for line in text.lines() {
                renderer.application.ui.console.push_line(line);
            }
        }
        FrontendAction::Help => {
            renderer.application.ui.console.push_ui_message(
                minimium_localization::UiMessage::Notice(
                    minimium_localization::UiNotice::CommandsHeader,
                ),
                &renderer.localizer,
                renderer.application.settings.locale,
            );
            for line in client_commands::help_lines() {
                renderer.application.ui.console.push_line(line);
            }
            renderer.application.ui.console.push_client_text(
                Text::ConsoleHistoryHelp,
                false,
                &renderer.localizer,
                renderer.application.settings.locale,
            );
        }
        FrontendAction::Connect(address) => {
            if renderer.application.connection.busy() {
                renderer.application.ui.console.push_client_text(
                    Text::DisconnectOrWait,
                    true,
                    &renderer.localizer,
                    renderer.application.settings.locale,
                );
            } else {
                renderer.start_remote_attempt(&address);
            }
        }
        FrontendAction::HostLocal(lane) => {
            if renderer.application.connection.busy() {
                renderer.application.ui.console.push_client_text(
                    Text::DisconnectOrWait,
                    true,
                    &renderer.localizer,
                    renderer.application.settings.locale,
                );
            } else {
                renderer.start_local_attempt(lane);
            }
        }
        FrontendAction::Disconnect => {
            renderer.application.connection.cancel();
            renderer.clear_connection();
            renderer.application.ui.console.push_client_text(
                Text::Disconnected,
                false,
                &renderer.localizer,
                renderer.application.settings.locale,
            );
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
            renderer.application.ui.console.push_client_text(
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
                &renderer.localizer,
                renderer.application.settings.locale,
            );
        }
        FrontendAction::State => renderer.application.ui.console.push_client_text(
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
            &renderer.localizer,
            renderer.application.settings.locale,
        ),
        FrontendAction::MapTeleport => renderer.arm_map_teleport(),
        FrontendAction::Clear => renderer.application.ui.console.clear_lines(),
        FrontendAction::SkyInfo => {
            for line in renderer.sky_info_lines() {
                renderer.application.ui.console.push_line(line);
            }
        }
        FrontendAction::SkyBodies => {
            let frame = &renderer.render_environment_frame;
            if frame.observed_bodies.is_empty() {
                renderer
                    .application
                    .ui
                    .console
                    .push_line(renderer.localizer.format(
                        renderer.application.settings.locale,
                        &minimium_localization::UiMessage::Notice(
                            minimium_localization::UiNotice::SkyBodiesUnavailable,
                        ),
                    ));
            } else {
                for body in frame
                    .observed_bodies
                    .iter()
                    .take(minimium_environment::universe::MAX_CELESTIAL_BODIES + 2)
                {
                    let flux: f64 = if body.id == minimium_astronomy::SKY_SUN_ID {
                        frame.local_sun.map_or(0.0, |local| {
                            f64::from(local.effective_linear_rgb.iter().sum::<f32>())
                        })
                    } else if body.id == minimium_astronomy::SKY_MOON_ID {
                        frame.local_moon.map_or(0.0, |local| {
                            f64::from(local.effective_linear_rgb.iter().sum::<f32>())
                        })
                    } else {
                        body.emitted_linear_rgb
                            .iter()
                            .chain(body.reflected_linear_rgb.iter())
                            .chain(body.scattered_linear_rgb.iter())
                            .sum()
                    };
                    renderer.application.ui.console.push_line(format!(
                        "body {} {:?} flux={flux:.4e} visible={}",
                        body.id.get(),
                        body.kind,
                        body.visible
                    ));
                }
            }
        }
        FrontendAction::SkyMeteorsDebug(mode) => super::meteor_diagnostics::debug(renderer, mode),
        FrontendAction::SkyMeteorsStatus => {
            super::meteor_diagnostics::status(renderer);
            renderer
                .application
                .ui
                .console
                .push_line(super::meteor_diagnostics::optical_status(renderer));
            for line in renderer.meteors.status_lines() {
                renderer.application.ui.console.push_line(line);
            }
        }
        FrontendAction::SkyMeteorsSpawn(preset, seed) => {
            let message = match renderer.meteors.spawn(preset, seed) {
                Ok(message) => message,
                Err(message) => message,
            };
            renderer.application.ui.console.push_line(message);
        }
        FrontendAction::SkyMeteorsLight(mode) => {
            renderer.gpu.transient_enabled = mode != crate::client_commands::MeteorLightDebug::Off;
            renderer.gpu.transient_transmission =
                mode == crate::client_commands::MeteorLightDebug::Transmission;
            renderer
                .application
                .ui
                .console
                .push_line(format!("meteors: local light {mode:?}"));
        }
        FrontendAction::SkyMeteorsClear => {
            let cleared = renderer.meteors.clear();
            renderer
                .application
                .ui
                .console
                .push_line(format!("meteors: cleared {cleared} active event(s)"));
        }
        FrontendAction::Property(command) => {
            for line in renderer.apply_property(command).lines() {
                renderer.application.ui.console.push_line(line);
            }
        }
        FrontendAction::ConfigSave(path) => {
            match crate::config::save_preferences(
                &path,
                &renderer.application.settings.config_file_settings,
                &renderer.session_settings(),
            ) {
                Ok(()) => renderer.application.ui.console.push_ui_message(
                    minimium_localization::UiMessage::ConfigurationSaved(
                        path.display().to_string(),
                    ),
                    &renderer.localizer,
                    renderer.application.settings.locale,
                ),
                Err(error) => {
                    renderer.application.connection.record_diagnostic_failure(
                        "storage.config_save_failed",
                        &error.to_string(),
                    );
                    renderer.application.ui.console.push_ui_error_message(
                        minimium_localization::UiMessage::ConfigurationSaveFailed,
                        &renderer.localizer,
                        renderer.application.settings.locale,
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
                    renderer.application.ui.console.push_ui_error_message(
                        minimium_localization::UiMessage::Notice(
                            minimium_localization::UiNotice::TimeRequestFailed,
                        ),
                        &renderer.localizer,
                        renderer.application.settings.locale,
                    );
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
                    renderer.application.ui.console.push_ui_error_message(
                        minimium_localization::UiMessage::Notice(
                            minimium_localization::UiNotice::TimeScaleRequestFailed,
                        ),
                        &renderer.localizer,
                        renderer.application.settings.locale,
                    );
                }
            }
        }
        FrontendAction::SpawnRandomBolid => {
            // Publish the current gaze before the separate admin request, also
            // when already connected and the camera just moved.
            renderer.pending_bolid_request = true;
            renderer.last_sky_view_at = None;
            renderer.bolid_notice(minimium_localization::UiNotice::BolidRequestQueued);
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
                    renderer.application.ui.console.push_ui_error_message(
                        minimium_localization::UiMessage::Notice(
                            minimium_localization::UiNotice::TimeModeRequestFailed,
                        ),
                        &renderer.localizer,
                        renderer.application.settings.locale,
                    );
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
                    renderer.application.ui.console.push_ui_error_message(
                        minimium_localization::UiMessage::Notice(
                            minimium_localization::UiNotice::TimeOfflinePolicyRequestFailed,
                        ),
                        &renderer.localizer,
                        renderer.application.settings.locale,
                    );
                }
            }
        }
        FrontendAction::Quit => return true,
    }
    false
}
