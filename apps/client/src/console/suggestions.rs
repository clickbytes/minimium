use super::editor::Editor;
use crate::client_commands::{command_specs, CommandSpec};

pub(super) struct Suggestions {
    choices: Vec<&'static str>,
    index: Option<usize>,
    suffix: String,
    start: usize,
    usage: Option<&'static str>,
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

    pub fn refresh(&mut self, editor: &Editor) {
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
        let specs = command_specs();
        self.usage = specs
            .iter()
            .find(|spec| matches_arguments(query, spec))
            .map(|spec| spec.usage);
        if self.usage.is_some() {
            return;
        }
        for spec in specs {
            if spec.name.starts_with(query) {
                self.choices.push(spec.name);
            }
            if let Some(alias) = spec.alias {
                if alias.starts_with(query) {
                    self.choices.push(alias);
                }
            }
        }
    }

    pub fn lines(&self) -> impl Iterator<Item = &'static str> + '_ {
        self.usage
            .into_iter()
            .chain(self.choices.iter().copied().take(5))
    }

    pub fn cycle(&mut self, editor: &mut Editor, reverse: bool) {
        if self.index.is_none() {
            self.refresh(editor);
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
        let replacement = self.choices[next];
        let old_end = editor.text.len() - self.suffix.len();
        if editor.text.len() - (old_end - self.start) + replacement.len() > super::theme::MAX_INPUT
        {
            return;
        }
        editor.replace_range(self.start..old_end, replacement);
        self.index = Some(next);
    }

    pub fn edited(&mut self, editor: &Editor) {
        self.index = None;
        self.refresh(editor);
    }
}

fn matches_arguments(input: &str, spec: &CommandSpec) -> bool {
    [Some(spec.name), spec.alias]
        .into_iter()
        .flatten()
        .any(|name| {
            input
                .strip_prefix(name)
                .is_some_and(|rest| rest.starts_with(char::is_whitespace))
        })
}
