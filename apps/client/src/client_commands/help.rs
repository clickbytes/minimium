use super::{command_specs, registry};

pub(crate) fn command_error(
    error: &minimium_commands::CommandError,
    localizer: &minimium_localization::Localizer,
    locale: minimium_localization::Locale,
) -> String {
    localizer.format(
        locale,
        &minimium_localization::UiMessage::CommandRejected {
            code: error.code.as_str().to_owned(),
            subject: error.subject.clone().unwrap_or_default(),
        },
    )
}

pub(crate) fn command_help(
    name: &str,
    localizer: &minimium_localization::Localizer,
    locale: minimium_localization::Locale,
) -> String {
    let Some(spec) = command_specs().iter().find(|spec| spec.name == name) else {
        return name.to_owned();
    };
    let translated = localizer.format(
        locale,
        &minimium_localization::UiMessage::CommandHelp(name.to_owned()),
    );
    let arguments = minimium_commands::format_command_help("", spec.arguments);
    format!("{translated}{arguments}")
}

pub(crate) fn command_help_group(
    prefix: &str,
    localizer: &minimium_localization::Localizer,
    locale: minimium_localization::Locale,
) -> String {
    command_specs()
        .iter()
        .filter(|spec| spec.name.starts_with(&format!("{prefix} ")))
        .map(|spec| command_help(spec.name, localizer, locale))
        .collect::<Vec<_>>()
        .join("\n")
}

pub(crate) fn help_lines() -> Vec<String> {
    let mut lines = vec!["Commands (use help COMMAND for details):".to_owned()];
    lines.extend(
        registry()
            .commands()
            .iter()
            .map(|spec| spec.usage.to_owned()),
    );
    lines.push("Properties: get KEY | set KEY VALUE | reset KEY | list [PREFIX]".into());
    lines
        .push("Use help KEY for property details. PageUp/PageDown or wheel scrolls output.".into());
    lines
}
