use super::super::DesktopHost;
use crate::client_commands::{CommandActionResult, CommandOutput};

pub(in crate::renderer) fn format_console_output(
    renderer: &mut DesktopHost,
    result: CommandActionResult,
) {
    for output in result.outputs {
        match output {
            CommandOutput::Line(line) => renderer.application.ui.console.push_line(line),
            CommandOutput::UiError(message) => {
                renderer.application.ui.console.push_ui_error_message(
                    message,
                    &renderer.localizer,
                    renderer.application.settings.locale,
                )
            }
            CommandOutput::ClientText { message, error } => {
                renderer.application.ui.console.push_client_text(
                    message,
                    error,
                    &renderer.localizer,
                    renderer.application.settings.locale,
                )
            }
            CommandOutput::Clear => renderer.application.ui.console.clear_lines(),
        }
    }
}
