use crate::client_commands::output as dispatch;
use minimium_localization::{Locale, Localizer};

pub(super) fn display(
    response: Option<dispatch::ClientResult>,
    localizer: &Localizer,
    locale: Locale,
) {
    let result = dispatch::display(response, localizer, locale);
    for output in result.outputs {
        match output {
            crate::client_commands::CommandOutput::Line(line) => {
                println!("{line}")
            }
            #[cfg(feature = "desktop")]
            crate::client_commands::CommandOutput::UiError(message) => {
                println!("{}", localizer.format(locale, &message))
            }
            crate::client_commands::CommandOutput::ClientText { message, .. } => {
                println!("{}", message.localized(localizer, locale));
            }
            crate::client_commands::CommandOutput::Clear => {
                print!("\x1B[2J\x1B[H")
            }
        }
    }
}
