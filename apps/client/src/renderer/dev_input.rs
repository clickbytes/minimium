//! Desktop execution and formatting for typed local developer presentation commands.
use super::DesktopHost;
use crate::client_commands::{output as dispatch, CommandOutput};

pub(super) fn execute(
    renderer: &mut DesktopHost,
    line: &crate::ConsoleInput,
) -> (bool, Option<dispatch::ClientResult>) {
    super::controller::execute_input(
        renderer,
        line.as_deref().map_err(Clone::clone),
        crate::client_commands::CommandSource::DeveloperStdin,
    )
}

pub(super) fn format_output(renderer: &mut DesktopHost, result: Option<dispatch::ClientResult>) {
    let result = dispatch::display(
        result,
        &renderer.localizer,
        renderer.application.settings.locale,
    );
    for output in result.outputs {
        match output {
            CommandOutput::Line(line) => println!("{line}"),
            CommandOutput::UiMessage(message) | CommandOutput::UiError(message) => println!(
                "{}",
                renderer
                    .localizer
                    .format(renderer.application.settings.locale, &message)
            ),
            CommandOutput::ClientText { message, .. } => println!(
                "{}",
                message.localized(&renderer.localizer, renderer.application.settings.locale)
            ),
            CommandOutput::Clear => {
                renderer.application.ui.console.clear_lines();
                print!("\x1b[2J\x1b[H");
            }
        }
    }
}
