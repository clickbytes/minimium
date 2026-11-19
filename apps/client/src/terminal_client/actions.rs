use super::{properties, ClientOptions};
use crate::application::ClientApplication;
use crate::client_commands::{CommandActionResult, CommandOutput, FrontendAction};
use crate::messages::ClientText as Text;
use minimium_localization::Localizer;

pub(super) fn handle(
    action: FrontendAction,
    options: &mut ClientOptions,
    application: &mut ClientApplication,
    localizer: &Localizer,
    text_properties: &mut properties::TextProperties,
) -> (bool, CommandActionResult) {
    let mut result = CommandActionResult::default();
    macro_rules! println {
        ($($argument:tt)*) => {
            {
                result.outputs.push(CommandOutput::Line(format!($($argument)*)));
            }
        };
    }
    match action {
        FrontendAction::Notice(notice) => println!(
            "{}",
            localizer.format(
                options.locale,
                &minimium_localization::UiMessage::Notice(notice),
            )
        ),
        FrontendAction::Print(text) => println!("{text}"),
        FrontendAction::Help | FrontendAction::Clear => unreachable!("shared console operation"),
        FrontendAction::Connect(_)
        | FrontendAction::HostLocal(_)
        | FrontendAction::Disconnect
        | FrontendAction::NetStats
        | FrontendAction::State
        | FrontendAction::Quit
        | FrontendAction::Ping => unreachable!("shared connection binding"),
        FrontendAction::FpsShow => {
            println!(
                "{}",
                Text::FpsUnavailable.localized(&localizer, options.locale)
            );
        }
        FrontendAction::MapTeleport => {
            println!(
                "{}",
                localizer.format(
                    options.locale,
                    &minimium_localization::UiMessage::Notice(
                        minimium_localization::UiNotice::MapTeleportDesktopOnly
                    )
                )
            );
        }
        FrontendAction::Property(command) => {
            match text_properties.execute(command, options, application.connection_view()) {
                Ok(line) => result.outputs.push(CommandOutput::Line(line)),
                Err(error) => {
                    result
                        .outputs
                        .push(CommandOutput::Line(crate::client_commands::command_error(
                            &error,
                            localizer,
                            options.locale,
                        )))
                }
            }
        }
        FrontendAction::CameraStatus
        | FrontendAction::CameraLook { .. }
        | FrontendAction::ConfigSave(_) => unreachable!("shared preference operation"),
        FrontendAction::Jump | FrontendAction::Movement(..) => {
            unreachable!("shared policy operations")
        }
        FrontendAction::Clock(_) => unreachable!("clock actions use the shared time adapter"),
        FrontendAction::SpawnRandomBolid(count) => {
            match application.request_random_bolid(count, crate::host_time::sample()) {
                Ok(()) => println!(
                    "{}",
                    localizer.format(
                        options.locale,
                        &minimium_localization::UiMessage::Notice(
                            minimium_localization::UiNotice::BolidRequestSent
                        )
                    )
                ),
                Err(error) => println!("spawn_random_bolid: {error}"),
            }
        }
        FrontendAction::SpawnRandomMeteorid => {
            match application.request_random_meteorid(crate::host_time::sample()) {
                Ok(()) => println!("Meteorite request sent"),
                Err(error) => println!("spawn_random_meteorid: {error}"),
            }
        }
        FrontendAction::Presentation(operation) => {
            let unavailable_result = crate::client_commands::PresentationUnavailable {
                capability: operation.capability(),
                requirement: operation.requirement(),
                reason: crate::client_commands::PresentationUnavailableReason::UnsupportedHost,
            };
            result.outputs.push(CommandOutput::Presentation(
                crate::client_commands::PresentationResult::Unavailable(unavailable_result),
            ));
        }
    }
    (false, result)
}
