mod action;
mod output;
mod presentation;
use super::*;
use crate::client_commands::output as dispatch;
pub(super) use output::format_console_output;

pub(super) fn execute_console_input(renderer: &mut DesktopHost) -> bool {
    let input = renderer
        .application
        .ui
        .console
        .take_input(&renderer.localizer, renderer.application.settings.locale);
    let (exit, result) = execute_input(renderer, &input, client_commands::CommandSource::Console);
    let output = dispatch::display(
        result,
        &renderer.localizer,
        renderer.application.settings.locale,
    );
    format_console_output(renderer, output);
    exit
}

pub(super) fn execute_input(
    renderer: &mut DesktopHost,
    input: &str,
    source: client_commands::CommandSource,
) -> (bool, Option<dispatch::ClientResult>) {
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
    dispatch::dispatch(result, |identity, action| {
        if let Some((result, changed)) = renderer
            .application
            .settings
            .values
            .locale_operation(&identity, &action)
        {
            if let Some(locale) = changed {
                renderer.apply_property_value(client_commands::ClientPropertyValue::Locale(locale));
            }
            return (false, dispatch::native(result));
        }
        let (exit, output) = action::execute(renderer, action);
        (
            exit,
            dispatch::legacy(
                identity,
                output,
                &renderer.localizer,
                renderer.application.settings.locale,
            ),
        )
    })
}
