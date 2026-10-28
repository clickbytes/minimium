use super::*;
use crate::messages::ClientText as Text;

pub(super) fn execute_console_input(renderer: &mut Renderer) -> bool {
    let input = renderer.console.take_input();
    let command = match client_commands::parse_command(&input) {
        Ok(Some(command)) => command,
        Ok(None) => return false,
        Err(error) => {
            renderer
                .console
                .push_error(Text::CommandParseError(error).to_string());
            return false;
        }
    };
    let direction = glam::Vec3::new(
        renderer.yaw.cos() * renderer.pitch.cos(),
        renderer.pitch.sin(),
        renderer.yaw.sin() * renderer.pitch.cos(),
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
            renderer.console.push_line(Text::CommandQueued.to_string());
            return false;
        }
        CommandResult::QueueError(error) => {
            renderer
                .console
                .push_error(Text::CouldNotQueueCommand(error.into()).to_string());
            return false;
        }
        CommandResult::Action(action) => action,
    };
    match action {
        FrontendAction::Help => {
            renderer.console.push_line(
                client_commands::help_lines()
                    .collect::<Vec<_>>()
                    .join(" | "),
            );
            renderer
                .console
                .push_line(Text::ConsoleHistoryHelp.to_string());
        }
        FrontendAction::Connect(address) => {
            if renderer.connection.busy() {
                renderer
                    .console
                    .push_error(Text::DisconnectOrWait.to_string());
            } else {
                renderer.start_remote_attempt(&address);
            }
        }
        FrontendAction::HostLocal(lane) => {
            if renderer.connection.busy() {
                renderer
                    .console
                    .push_error(Text::DisconnectOrWait.to_string());
            } else {
                renderer.start_local_attempt(lane.unwrap_or(renderer.threads));
            }
        }
        FrontendAction::Disconnect => {
            renderer.connection.cancel();
            renderer.clear_connection();
            renderer.console.push_line(Text::Disconnected.to_string());
        }
        FrontendAction::FpsShow => renderer.console.push_line(renderer.frame_stats.summary()),
        FrontendAction::NetStats => {
            let metrics = renderer
                .connection
                .session()
                .map(ClientSession::metrics)
                .unwrap_or_default();
            renderer.console.push_line(format!(
                "{}",
                Text::NetworkStats {
                    bytes_sent: renderer
                        .previous_bytes_sent
                        .saturating_add(metrics.bytes_sent),
                    bytes_received: renderer
                        .previous_bytes_received
                        .saturating_add(metrics.bytes_received),
                    ping_milliseconds: metrics.ping_round_trip.map(|duration| duration.as_millis()),
                    nearby_players: Some(renderer.remote_player_count),
                }
            ));
        }
        FrontendAction::State => renderer.console.push_line(
            Text::PlayerState {
                position: renderer.character.position,
                health: renderer.health,
                nearby_players: renderer.remote_player_count,
            }
            .to_string(),
        ),
        FrontendAction::Clear => renderer.console.clear_lines(),
        FrontendAction::Quit => return true,
    }
    false
}
