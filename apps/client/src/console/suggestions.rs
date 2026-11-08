use super::editor::Editor;
use crate::client_commands::{command_specs, property_spec, property_specs, CommandSpec};
use minimium_commands::PropertyType;

pub(super) struct Suggestions {
    choices: Vec<String>,
    index: Option<usize>,
    suffix: String,
    start: usize,
    usage: Option<String>,
}

impl Suggestions {
    pub fn new() -> Self {
        Self {
            choices: Vec::new(),
            index: None,
            suffix: String::new(),
            start: 0,
            usage: None,
        }
    }

    pub fn refresh(
        &mut self,
        editor: &Editor,
        localizer: &minimium_localization::Localizer,
        locale: minimium_localization::Locale,
    ) {
        if self.index.is_some() {
            return;
        }
        self.choices.clear();
        self.usage = None;
        let prefix = &editor.text[..editor.cursor];
        let start = prefix.len() - prefix.trim_start().len();
        let query = &prefix[start..];
        if query.is_empty() {
            return;
        }
        if property_suggestions(self, query, localizer, locale) {
            return;
        }
        let specs = command_specs();
        if command_argument_suggestions(self, query, specs, localizer, locale) {
            return;
        }
        self.usage = specs
            .iter()
            .find(|spec| matches_arguments(query, spec))
            .map(|spec| crate::client_commands::command_help(spec.name, localizer, locale));
        if self.usage.is_some() {
            return;
        }
        for spec in specs {
            if spec.name.starts_with(query) {
                self.choices.push(spec.name.to_owned());
            }
            if let Some(alias) = spec.alias.filter(|alias| alias.starts_with(query)) {
                self.choices.push(alias.to_owned());
            }
        }
    }

    pub fn lines(&self) -> Vec<String> {
        let mut lines = self.usage.iter().cloned().collect::<Vec<_>>();
        lines.extend(self.choices.iter().take(5).cloned());
        lines
    }

    pub fn cycle(
        &mut self,
        editor: &mut Editor,
        reverse: bool,
        localizer: &minimium_localization::Localizer,
        locale: minimium_localization::Locale,
    ) {
        if self.index.is_none() {
            self.refresh(editor, localizer, locale);
            if self.choices.is_empty() {
                return;
            }
            self.start = editor.cursor - editor.text[..editor.cursor].trim_start().len();
            self.suffix = editor.text[editor.cursor..].to_owned();
        }
        let count = self.choices.len();
        let next = match self.index {
            Some(index) if reverse => (index + count - 1) % count,
            Some(index) => (index + 1) % count,
            None if reverse => count - 1,
            None => 0,
        };
        let replacement = self.choices[next].clone();
        let old_end = editor.text.len() - self.suffix.len();
        if editor.text.len() - (old_end - self.start) + replacement.len() > super::theme::MAX_INPUT
        {
            return;
        }
        editor.replace_range(self.start..old_end, &replacement);
        self.index = Some(next);
    }

    pub fn edited(
        &mut self,
        editor: &Editor,
        localizer: &minimium_localization::Localizer,
        locale: minimium_localization::Locale,
    ) {
        self.index = None;
        self.refresh(editor, localizer, locale);
    }
}

fn property_suggestions(
    suggestions: &mut Suggestions,
    query: &str,
    localizer: &minimium_localization::Localizer,
    locale: minimium_localization::Locale,
) -> bool {
    let words = query.split_whitespace().collect::<Vec<_>>();
    let Some(verb) = words.first().copied() else {
        return false;
    };
    if !matches!(verb, "get" | "set" | "reset" | "list" | "help") {
        return false;
    }
    let trailing_space = query.chars().last().is_some_and(char::is_whitespace);
    let key_position = matches!(verb, "get" | "set" | "reset" | "help");
    if key_position && words.len() <= 2 && !(words.len() == 2 && trailing_space) {
        let prefix = words.get(1).copied().unwrap_or_default();
        suggestions.choices.extend(
            property_specs()
                .filter(|spec| spec.key.starts_with(prefix) && (verb != "reset" || spec.resettable))
                .map(|spec| spec.key.to_owned()),
        );
        if verb == "help" {
            suggestions.choices.extend(
                command_specs()
                    .iter()
                    .filter(|spec| spec.name.starts_with(prefix))
                    .map(|spec| spec.name.to_owned()),
            );
        }
        if let Some(spec) = property_spec(prefix) {
            suggestions.usage = Some(crate::client_commands::format_property_help_localized(
                spec, localizer, locale,
            ));
        } else if verb == "help" {
            suggestions.usage = command_specs()
                .iter()
                .find(|spec| spec.name == prefix || spec.alias == Some(prefix))
                .map(|spec| crate::client_commands::command_help(spec.name, localizer, locale));
        }
        return true;
    }
    if verb == "list" && words.len() <= 2 {
        let prefix = words.get(1).copied().unwrap_or_default();
        suggestions.choices.extend(
            property_specs()
                .filter(|spec| spec.key.starts_with(prefix))
                .map(|spec| spec.key.to_owned()),
        );
        return true;
    }
    if verb == "set" && words.len() >= 2 {
        let Some(spec) = property_spec(words[1]) else {
            return true;
        };
        if words.len() >= 3 || trailing_space {
            let prefix = if trailing_space {
                ""
            } else {
                words.last().copied().unwrap_or_default()
            };
            if matches!(
                spec.value_type,
                PropertyType::Boolean | PropertyType::Enumeration
            ) {
                if let Some(constraints) = spec.constraints {
                    suggestions.choices.extend(
                        constraints
                            .split('|')
                            .filter(|value| value.starts_with(prefix))
                            .map(str::to_owned),
                    );
                }
            }
            if suggestions.choices.is_empty() {
                suggestions.usage = Some(crate::client_commands::format_property_help_localized(
                    spec, localizer, locale,
                ));
            }
        }
        return true;
    }
    true
}

fn matches_arguments(input: &str, spec: &CommandSpec) -> bool {
    [Some(spec.name), spec.alias]
        .into_iter()
        .flatten()
        .any(|name| {
            input
                .strip_prefix(name)
                .is_some_and(|rest| rest.is_empty() || rest.starts_with(char::is_whitespace))
        })
}

fn command_argument_suggestions(
    suggestions: &mut Suggestions,
    query: &str,
    specs: &[CommandSpec],
    localizer: &minimium_localization::Localizer,
    locale: minimium_localization::Locale,
) -> bool {
    let Ok(words) = minimium_commands::lex_command_line(query) else {
        return false;
    };
    let Some(spec) = specs
        .iter()
        .filter(|spec| matches_arguments(query, spec))
        .max_by_key(|spec| spec.name.len())
    else {
        return false;
    };
    let command = [Some(spec.name), spec.alias]
        .into_iter()
        .flatten()
        .find(|name| query.starts_with(name))
        .unwrap_or(spec.name);
    let entered_arguments = words
        .len()
        .saturating_sub(command.split_whitespace().count());
    let trailing_space = query.chars().last().is_some_and(char::is_whitespace);
    let argument_index = if trailing_space {
        entered_arguments
    } else {
        entered_arguments.saturating_sub(1)
    };
    let current = if trailing_space {
        ""
    } else {
        words.last().map(String::as_str).unwrap_or("")
    };
    if let Some(argument) = spec.arguments.get(argument_index) {
        if let Some(choices) = argument.choices {
            suggestions.choices.extend(
                choices
                    .iter()
                    .filter(|choice| choice.starts_with(current))
                    .map(|choice| (*choice).to_owned()),
            );
        }
    }
    suggestions.usage = Some(crate::client_commands::command_help(
        spec.name, localizer, locale,
    ));
    true
}
