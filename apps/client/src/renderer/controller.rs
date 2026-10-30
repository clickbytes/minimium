use super::*;
use crate::messages::ClientText as Text;

pub(super) fn execute_console_input(renderer: &mut Renderer) -> bool {
    let input = renderer.ui.console.take_input();
    let command = match client_commands::parse_command(&input) {
        Ok(Some(command)) => command,
        Ok(None) => return false,
        Err(error) => {
            renderer
                .ui
                .console
                .push_error(Text::CommandParseError(error).to_string());
            return false;
        }
    };
    let direction = super::camera::camera_direction(
        renderer.game.character,
        renderer.input.yaw,
        renderer.input.pitch,
    )
    .normalize()
    .to_array();
    let result = client_commands::execute(
        command,
        &mut CommandContext {
            connection: &mut renderer.connection,
            fire_direction: direction,
        },
    );
    let action = match result {
        CommandResult::Sent => {
            renderer
                .ui
                .console
                .push_line(Text::CommandQueued.to_string());
            return false;
        }
        CommandResult::QueueError(error) => {
            renderer
                .ui
                .console
                .push_error(Text::CouldNotQueueCommand(error.into()).to_string());
            return false;
        }
        CommandResult::Action(action) => action,
    };
    match action {
        FrontendAction::Help => {
            renderer
                .ui
                .console
                .push_line("Commands (PageUp/PageDown to scroll):");
            for line in client_commands::help_lines() {
                renderer.ui.console.push_line(line);
            }
            renderer
                .ui
                .console
                .push_line(Text::ConsoleHistoryHelp.to_string());
        }
        FrontendAction::Connect(address) => {
            if renderer.connection.busy() {
                renderer
                    .ui
                    .console
                    .push_error(Text::DisconnectOrWait.to_string());
            } else {
                renderer.start_remote_attempt(&address);
            }
        }
        FrontendAction::HostLocal(lane) => {
            if renderer.connection.busy() {
                renderer
                    .ui
                    .console
                    .push_error(Text::DisconnectOrWait.to_string());
            } else {
                renderer.start_local_attempt(lane.unwrap_or(renderer.settings.threads));
            }
        }
        FrontendAction::Disconnect => {
            renderer.connection.cancel();
            renderer.clear_connection();
            renderer
                .ui
                .console
                .push_line(Text::Disconnected.to_string());
        }
        FrontendAction::FpsShow => renderer
            .ui
            .console
            .push_line(renderer.metrics.frame_stats.summary()),
        FrontendAction::NetStats => {
            let metrics = renderer
                .connection
                .session()
                .map(ClientSession::metrics)
                .unwrap_or_default();
            renderer.ui.console.push_line(format!(
                "{}",
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
                    nearby_players: Some(renderer.game.remote_player_count),
                }
            ));
        }
        FrontendAction::State => renderer.ui.console.push_line(
            Text::PlayerState {
                position: renderer.game.character.position,
                health: renderer.game.health,
                nearby_players: renderer.game.remote_player_count,
            }
            .to_string(),
        ),
        FrontendAction::MapTeleport => renderer.arm_map_teleport(),
        FrontendAction::Clear => renderer.ui.console.clear_lines(),
        FrontendAction::SkyQuality(quality) => {
            let status = renderer.set_sky_quality(quality);
            renderer.ui.console.push_line(status);
        }
        FrontendAction::SkyInfo => {
            for line in renderer.sky_info_lines() {
                renderer.ui.console.push_line(line);
            }
        }
        FrontendAction::SkyBodies => {
            let frame = &renderer.render_environment_frame;
            if frame.observed_bodies.is_empty() {
                renderer.ui.console.push_line("sky bodies unavailable");
            } else {
                for body in frame
                    .observed_bodies
                    .iter()
                    .take(minimium_core::universe::MAX_CELESTIAL_BODIES + 2)
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
                    renderer.ui.console.push_line(format!(
                        "body {} {:?} flux={flux:.4e} visible={}",
                        body.id.get(),
                        body.kind,
                        body.visible
                    ));
                }
            }
        }
        FrontendAction::SkyDemo(enabled) => {
            renderer.demo_universe_enabled = enabled;
            renderer.ui.console.push_line(if enabled {
                "local sky demo enabled"
            } else {
                "local sky demo disabled"
            });
        }
        FrontendAction::Fov(degrees) => {
            if let Some(degrees) = degrees {
                renderer.fov_degrees = degrees;
            }
            renderer
                .ui
                .console
                .push_line(format!("vertical FOV: {:.1}°", renderer.fov_degrees));
        }
        FrontendAction::ShowCelestialNames(enabled) => {
            renderer.show_celestial_names = enabled;
            renderer.ui.console.push_line(if enabled {
                "celestial names enabled"
            } else {
                "celestial names disabled"
            });
        }
        FrontendAction::TimeStatus => renderer
            .ui
            .console
            .push_line(renderer.environment_frame.time_status_line()),
        FrontendAction::TimeScale(value) => match renderer.connection.request_time_scale(value) {
            Ok(()) => renderer.ui.console.push_line("time scale request sent"),
            Err(error) => renderer.ui.console.push_error(error),
        },
        FrontendAction::Quit => return true,
    }
    false
}
