use super::{catalog::catalog, command_specs};
use minimium_commands::operation::CatalogEntry;

pub(crate) fn command_error(
    error: &minimium_commands::CommandError,
    localizer: &minimium_localization::Localizer,
    locale: minimium_localization::Locale,
) -> String {
    let header = localizer.format(
        locale,
        &minimium_localization::UiMessage::CommandRejected {
            code: error.code.as_str().to_owned(),
            subject: error.subject.clone().unwrap_or_default(),
        },
    );
    match &error.usage {
        Some(usage) => format!("{header}: {}\n{usage}", error.reason),
        None => format!("{header}: {}", error.reason),
    }
}

pub(crate) fn command_help(
    name: &str,
    localizer: &minimium_localization::Localizer,
    locale: minimium_localization::Locale,
) -> String {
    let Some(entry) = catalog().resolve(name) else {
        return name.to_owned();
    };
    if let Some(spec) = command_specs()
        .iter()
        .find(|s| s.name == name || s.alias == Some(name))
    {
        let translated = localizer.format(
            locale,
            &minimium_localization::UiMessage::CommandHelp(spec.name.to_owned()),
        );
        format!("{translated}\n{}", entry.describe())
    } else {
        entry.describe()
    }
}

pub(super) fn group_entries(prefix: &str) -> Vec<&'static CatalogEntry> {
    let words = format!("{prefix} ");
    let path = format!("{prefix}.");
    catalog()
        .entries()
        .filter(|e| {
            e.spec.path.starts_with(&path) || e.aliases.iter().any(|a| a.name.starts_with(&words))
        })
        .collect()
}

pub(crate) fn command_help_group(
    prefix: &str,
    localizer: &minimium_localization::Localizer,
    locale: minimium_localization::Locale,
) -> String {
    group_entries(prefix)
        .iter()
        .map(|entry| command_help(&entry.spec.path, localizer, locale))
        .collect::<Vec<_>>()
        .join("\n")
}

pub(crate) fn help_lines() -> Vec<String> {
    let mut lines = vec!["Commands (use help COMMAND for details):".to_owned()];
    lines.extend(catalog().entries().map(|entry| {
        let names = entry
            .aliases
            .iter()
            .map(|a| a.name)
            .collect::<Vec<_>>()
            .join(", ");
        if names.is_empty() {
            entry.spec.path.clone()
        } else {
            format!("{} ({names})", entry.spec.path)
        }
    }));
    lines.push("Properties: get KEY | set KEY VALUE | reset KEY | list [PREFIX]".into());
    lines
        .push("Use help KEY for property details. PageUp/PageDown or wheel scrolls output.".into());
    #[cfg(feature = "desktop")]
    lines.push("Developer stdin (--dev --dev-stdin) accepts bounded commands during loading; scene-bound operations explain when presentation is unavailable.".into());
    lines
}
