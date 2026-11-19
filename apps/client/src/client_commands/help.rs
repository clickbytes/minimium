use super::{catalog::catalog, ClientPropertyCommand, FrontendAction};
use minimium_commands::operation::CatalogEntry;
use minimium_commands::operation::OperationId;
use minimium_localization::{Locale, Localizer, UiMessage, UiNotice};

pub(crate) fn command_error(
    error: &minimium_commands::CommandError,
    localizer: &minimium_localization::Localizer,
    locale: minimium_localization::Locale,
) -> String {
    if error.code == minimium_commands::CommandErrorCode::InputTooLong {
        return input_length_error(error.span, localizer, locale);
    }
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
    if let Some(p) = entry.property {
        return super::format_property_help_localized(p, localizer, locale);
    }
    let description = entry
        .command
        .map(|name| localizer.format(locale, &UiMessage::CommandHelp(name.into())))
        .unwrap_or_default();
    let kind = localizer.format(locale, &UiMessage::Notice(UiNotice::CompletionMethod));
    format!("{kind} {description}\n{}", entry.describe())
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

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct CatalogHelp {
    entries: Vec<OperationId>,
    mode: HelpMode,
    pub truncated: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum HelpMode {
    Index,
    Detail,
    Properties,
}

impl CatalogHelp {
    pub(super) fn for_action(action: &FrontendAction) -> Option<Self> {
        use ClientPropertyCommand as Property;
        let (mode, entries): (HelpMode, Box<dyn Iterator<Item = &CatalogEntry>>) = match action {
            FrontendAction::Help => (HelpMode::Index, Box::new(catalog().entries())),
            FrontendAction::Property(Property::Help(key)) => (
                HelpMode::Detail,
                Box::new(catalog().resolve(key.key()).into_iter()),
            ),
            FrontendAction::Property(Property::CommandHelp(name)) => (
                HelpMode::Detail,
                Box::new(catalog().resolve(name).into_iter()),
            ),
            FrontendAction::Property(Property::CommandHelpGroup(group)) => {
                (HelpMode::Detail, Box::new(group_entries(group).into_iter()))
            }
            FrontendAction::Property(Property::List(prefix)) => (
                HelpMode::Properties,
                Box::new(catalog().entries().filter(move |e| {
                    e.property
                        .is_some_and(|p| prefix.as_ref().is_none_or(|s| p.key.starts_with(s)))
                })),
            ),
            _ => return None,
        };
        let mut iter = entries.map(|e| e.spec.id.clone());
        let entries = iter.by_ref().take(128).collect();
        Some(Self {
            mode,
            entries,
            truncated: iter.next().is_some(),
        })
    }

    pub(super) fn format(&self, localizer: &Localizer, locale: Locale) -> Vec<String> {
        let mut lines = Vec::new();
        if self.mode == HelpMode::Index {
            lines.push(localizer.format(locale, &UiMessage::Notice(UiNotice::CommandsHeader)));
        }
        for id in &self.entries {
            let Some(e) = catalog().by_id(id) else {
                continue;
            };
            match self.mode {
                HelpMode::Detail => lines.extend(
                    command_help(&e.spec.path, localizer, locale)
                        .lines()
                        .map(str::to_owned),
                ),
                HelpMode::Properties => {
                    if let Some(p) = e.property {
                        lines.push(format!(
                            "{} — {}",
                            p.key,
                            localizer.format(locale, &UiMessage::PropertyDescription(p.key.into()))
                        ));
                    }
                }
                HelpMode::Index => {
                    let aliases = e
                        .aliases
                        .iter()
                        .map(|a| a.name)
                        .collect::<Vec<_>>()
                        .join(", ");
                    lines.push(if aliases.is_empty() {
                        e.spec.path.clone()
                    } else {
                        format!("{} ({aliases})", e.spec.path)
                    });
                }
            }
        }
        if self.entries.is_empty() {
            lines.push(localizer.format(locale, &UiMessage::NoPropertyMatches("*".into())));
        }
        if self.truncated {
            lines.push("…".into());
        }
        if self.mode == HelpMode::Index {
            lines.push(localizer.format(locale, &UiMessage::ConsoleHistoryHelp));
        }
        lines
    }
}

pub(super) fn input_length_error(
    span: Option<minimium_commands::SourceSpan>,
    localizer: &Localizer,
    locale: Locale,
) -> String {
    localizer.format(
        locale,
        &UiMessage::InputTooLong {
            limit: minimium_commands::MAX_INPUT_BYTES,
            received: span.map_or(0, |s| s.end),
        },
    )
}
