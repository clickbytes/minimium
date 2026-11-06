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
        renderer.game.gameplay.prediction.character,
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
        FrontendAction::AtmosphereDebug(mode) => {
            renderer.gpu.terrain_gpu.uniform.debug[0] = 0;
            renderer.gpu.atmosphere_gpu.debug = mode as u32;
            renderer
                .ui
                .console
                .push_line(format!("atmosphere debug {mode:?}"));
        }
        FrontendAction::AtmosphereStatus => {
            for line in renderer.gpu.atmosphere_gpu.status() {
                renderer.ui.console.push_line(line);
            }
        }
        FrontendAction::WeatherStatus => {
            for line in renderer.weather_status() {
                renderer.ui.console.push_line(line);
            }
        }
        FrontendAction::TerrainStatus => {
            for line in renderer.terrain_status() {
                renderer.ui.console.push_line(line);
            }
        }
        FrontendAction::TerrainDebug(mode) => {
            renderer.gpu.atmosphere_gpu.debug = 0;
            renderer.gpu.terrain_gpu.uniform.debug[0] = mode as u32;
            renderer.gpu.terrain_gpu.update(&renderer.gpu.queue);
            renderer
                .ui
                .console
                .push_line(format!("terrain debug {mode:?}"));
        }
        FrontendAction::WeatherDebug(mode) => {
            renderer.weather.debug = mode;
            renderer
                .ui
                .console
                .push_line(format!("weather clouds debug {mode:?}"));
        }
        FrontendAction::WeatherDetail(level) => {
            renderer.weather.set_detail(level);
            renderer.ui.console.push_line(format!(
                "weather clouds detail N={level} (thin-layer minimum retained)"
            ));
        }
        FrontendAction::Print(text) => {
            for line in text.lines() {
                renderer.ui.console.push_line(line);
            }
        }
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
                    nearby_players: Some(renderer.game.gameplay.reconciliation.player_count()),
                }
            ));
        }
        FrontendAction::State => renderer.ui.console.push_line(
            Text::PlayerState {
                position: renderer.game.gameplay.prediction.character.position,
                health: renderer.game.gameplay.health,
                nearby_players: renderer.game.gameplay.reconciliation.player_count(),
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
        FrontendAction::SetTime { minute, local } => {
            let [x, _, z] = renderer.game.view().player_position_m;
            let result = if local && (!renderer.is_earth_world() || x.hypot(z) < 1.0e-6) {
                Err("local solar time requires an Earth observer away from the poles".into())
            } else {
                renderer
                    .connection
                    .request_set_time(minute, if local { z.atan2(x) } else { 0.0 })
            };
            match result {
                Ok(()) => renderer.ui.console.push_line("time change request sent"),
                Err(error) => renderer.ui.console.push_error(error),
            }
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
