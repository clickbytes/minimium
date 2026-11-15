use super::super::Renderer;
use crate::{
    client_commands::{self, CommandActionResult, CommandOutput},
    messages::ClientText,
};

#[derive(Default)]
pub(super) struct CommandOutputCollector {
    pub(super) result: CommandActionResult,
}

impl CommandOutputCollector {
    pub(super) fn push_line(&mut self, line: impl Into<String>) {
        self.result.outputs.push(CommandOutput::Line(line.into()));
    }

    pub(super) fn push_ui_message(&mut self, message: minimium_localization::UiMessage) {
        self.result.outputs.push(CommandOutput::UiMessage(message));
    }

    pub(super) fn push_ui_error_message(&mut self, message: minimium_localization::UiMessage) {
        self.result.outputs.push(CommandOutput::UiError(message));
    }

    pub(super) fn push_client_text(&mut self, message: ClientText, error: bool) {
        self.result
            .outputs
            .push(CommandOutput::ClientText { message, error });
    }

    pub(super) fn clear_lines(&mut self) {
        self.result.outputs.push(CommandOutput::Clear);
    }
}

pub(in crate::renderer) fn format_console_output(
    renderer: &mut Renderer,
    result: CommandActionResult,
) {
    for output in result.outputs {
        match output {
            CommandOutput::Line(line) => renderer.application.ui.console.push_line(line),
            CommandOutput::UiMessage(message) => renderer.application.ui.console.push_ui_message(
                message,
                &renderer.localizer,
                renderer.application.settings.locale,
            ),
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
            CommandOutput::Presentation(result) => match result {
                #[cfg(feature = "desktop")]
                client_commands::PresentationResult::Output(lines) => {
                    for line in lines {
                        renderer.application.ui.console.push_line(line);
                    }
                }
                client_commands::PresentationResult::Unavailable(_) => {
                    renderer.application.ui.console.push_ui_message(
                        minimium_localization::UiMessage::Notice(
                            minimium_localization::UiNotice::WeatherRequiresDesktop,
                        ),
                        &renderer.localizer,
                        renderer.application.settings.locale,
                    )
                }
            },
            CommandOutput::Clear => renderer.application.ui.console.clear_lines(),
        }
    }
}
