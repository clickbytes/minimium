mod action;
mod output;
mod presentation;
use super::*;
use crate::messages::ClientText as Text;
pub(super) use output::format_console_output;
use output::CommandOutputCollector;

pub(super) fn execute_console_input(renderer: &mut DesktopHost) -> bool {
    let input = renderer
        .application
        .ui
        .console
        .take_input(&renderer.localizer, renderer.application.settings.locale);
    let (exit, result) = execute_input(renderer, &input, client_commands::CommandSource::Console);
    format_console_output(renderer, result);
    exit
}

pub(super) fn execute_input(
    renderer: &mut DesktopHost,
    input: &str,
    source: client_commands::CommandSource,
) -> (bool, client_commands::CommandActionResult) {
    let direction = super::camera::camera_direction(
        renderer.application.core_view().character,
        renderer.input.yaw,
        renderer.input.pitch,
    )
    .normalize()
    .to_array();
    let player_position = renderer.application.core_view().character.position;
    let result = renderer.application.execute_text_command(
        input,
        source,
        direction,
        Some(player_position),
        crate::host_time::sample(),
    );
    let mut output = CommandOutputCollector::default();
    let action = match result {
        client_commands::TextCommandResult::Empty => return (false, output.result),
        client_commands::TextCommandResult::Rejected(error) => {
            output.push_ui_error_message(minimium_localization::UiMessage::CommandRejected {
                code: error.code.as_str().to_owned(),
                subject: error.subject.clone().unwrap_or_default(),
            });
            if let Some(usage) = error.usage.as_deref() {
                output.push_line(format!("Usage: {usage}"));
            }
            return (false, output.result);
        }
        client_commands::TextCommandResult::Queued { .. } => {
            output.push_client_text(Text::CommandQueued, false);
            return (false, output.result);
        }
        client_commands::TextCommandResult::QueueRejected { error, .. } => {
            output.push_client_text(Text::CouldNotQueueCommand(error.into()), true);
            return (false, output.result);
        }
        client_commands::TextCommandResult::Suppressed(_) => return (false, output.result),
        client_commands::TextCommandResult::Action(action) => action,
    };
    action::execute(renderer, action)
}
