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
            crate::client_commands::CommandOutput::UiMessage(message)
            | crate::client_commands::CommandOutput::UiError(message) => {
                println!("{}", localizer.format(locale, &message))
            }
            crate::client_commands::CommandOutput::ClientText { message, .. } => {
                println!("{}", message.localized(localizer, locale));
            }
            crate::client_commands::CommandOutput::Clear => {
                print!("\x1B[2J\x1B[H")
            }
            crate::client_commands::CommandOutput::Presentation(result) => match result {
                #[cfg(feature = "desktop")]
                crate::client_commands::PresentationResult::Output(lines) => {
                    for line in lines {
                        println!("{line}");
                    }
                }
                crate::client_commands::PresentationResult::Unavailable(unavailable) => {
                    if unavailable.reason
                        != crate::client_commands::PresentationUnavailableReason::UnsupportedHost
                    {
                        println!("{}", unavailable.message());
                        continue;
                    }
                    let notice = match unavailable.capability {
                        crate::client_commands::PresentationCapability::DeveloperRenderer
                        | crate::client_commands::PresentationCapability::WeatherDiagnostics => {
                            minimium_localization::UiNotice::WeatherRequiresDesktop
                        }
                        #[cfg(feature = "desktop")]
                        crate::client_commands::PresentationCapability::EarthMapTeleport => {
                            minimium_localization::UiNotice::MapTeleportDesktopOnly
                        }
                        crate::client_commands::PresentationCapability::SkyDiagnostics => {
                            minimium_localization::UiNotice::SkyInfoDesktopOnly
                        }
                        crate::client_commands::PresentationCapability::MeteorDiagnostics => {
                            minimium_localization::UiNotice::MeteorsRequireDesktop
                        }
                    };
                    println!(
                        "{}",
                        localizer.format(locale, &minimium_localization::UiMessage::Notice(notice))
                    );
                }
            },
        }
    }
}
