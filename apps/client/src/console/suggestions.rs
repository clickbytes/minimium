use super::editor::Editor;
use crate::client_commands::analyze_console;
use minimium_commands::completion::CompletionResult;
use minimium_localization::{Locale, Localizer};
use std::ops::Range;

pub(super) struct Suggestions {
    result: Option<CompletionResult>,
    cycle: Option<(usize, u64, Range<usize>)>,
}

impl Suggestions {
    pub fn new() -> Self {
        Self {
            result: None,
            cycle: None,
        }
    }

    pub fn refresh(&mut self, editor: &Editor, _: &Localizer, _: Locale) {
        self.result = Some(analyze_console(
            &editor.text,
            editor.cursor,
            editor.selection(),
            editor.revision,
        ));
        self.cycle = None;
    }

    pub fn lines(&self) -> Vec<String> {
        let Some(result) = &self.result else {
            return Vec::new();
        };
        let mut lines = Vec::new();
        if let Some(s) = &result.signature {
            let args = s
                .arguments
                .iter()
                .map(|p| {
                    format!(
                        "{}{}: {}",
                        p.name,
                        if p.optional { "?" } else { "" },
                        p.value_type.0
                    )
                })
                .collect::<Vec<_>>()
                .join(", ");
            lines.push(format!("{}({}) -> {}", s.operation.0, args, s.result.0));
        }
        lines.extend(result.items.iter().take(5).map(|i| i.label.clone()));
        lines
    }

    pub fn cycle(
        &mut self,
        editor: &mut Editor,
        reverse: bool,
        localizer: &Localizer,
        locale: Locale,
    ) {
        if !self
            .cycle
            .as_ref()
            .is_some_and(|(_, rev, _)| *rev == editor.revision)
        {
            self.refresh(editor, localizer, locale);
        }
        let Some(result) = &self.result else { return };
        let count = result.items.len();
        if count == 0 {
            return;
        }
        let next = match &self.cycle {
            Some((i, _, _)) if reverse => (i + count - 1) % count,
            Some((i, _, _)) => (i + 1) % count,
            None if reverse => count - 1,
            None => 0,
        };
        let item = &result.items[next];
        let range = self
            .cycle
            .as_ref()
            .map_or_else(|| item.replace_range.clone(), |(_, _, r)| r.clone());
        let start = range.start;
        let rev = editor.revision;
        editor.replace_range(range, &item.insert_text);
        if rev != editor.revision {
            editor.move_to(start + item.caret_offset, false);
            self.cycle = Some((next, editor.revision, start..start + item.insert_text.len()));
        }
    }

    pub fn edited(&mut self, editor: &Editor, localizer: &Localizer, locale: Locale) {
        self.refresh(editor, localizer, locale);
    }
}
