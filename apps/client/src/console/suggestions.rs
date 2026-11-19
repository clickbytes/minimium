use super::{editor::Editor, signature_help};
use crate::client_commands::{analyze_console, validate_edit};
use minimium_commands::completion::{CompletionResult, ItemKind};
use minimium_commands::SourceSpan;
use minimium_localization::{Locale, Localizer, UiMessage, UiNotice};

#[derive(Default)]
pub(super) struct Suggestions {
    pub result: Option<CompletionResult>,
    pub labels: Vec<String>,
    pub signature: Option<String>,
    pub diagnostic: Option<(String, SourceSpan)>,
    pub selected: usize,
    pub open: bool,
}

impl Suggestions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn edited(&mut self, editor: &Editor, localizer: &Localizer, locale: Locale) {
        let result = analyze_console(
            &editor.text,
            editor.cursor,
            editor.selection(),
            editor.revision,
        );
        self.diagnostic = None;
        self.selected = 0;
        self.labels = result
            .items
            .iter()
            .map(|item| {
                let (notice, access) = match item.kind {
                    ItemKind::Property => (
                        UiNotice::CompletionField,
                        if item.writable { "RW" } else { "RO" },
                    ),
                    ItemKind::Method => (
                        UiNotice::CompletionMethod,
                        item.access.map_or("", |a| a.permission),
                    ),
                    ItemKind::Namespace => (UiNotice::CompletionNamespace, ""),
                    ItemKind::Argument => (
                        UiNotice::CompletionArgument,
                        item.access.map_or("", |a| a.permission),
                    ),
                };
                let label = localizer.format(locale, &UiMessage::Notice(notice));
                let signature = item
                    .signature
                    .as_ref()
                    .map(signature_help::declaration)
                    .unwrap_or_default();
                format!(
                    "{label} {}{signature} {} {} {access}",
                    item.label,
                    item.value_type.map_or("", |t| t.0),
                    item.unit.unwrap_or("")
                )
                .trim_end()
                .chars()
                .take(192)
                .collect()
            })
            .collect();
        self.signature = result.signature.as_ref().map(signature_help::active);
        self.open = !editor.text.is_empty()
            && !editor.is_composing()
            && (!self.labels.is_empty() || self.signature.is_some());
        self.result = Some(result);
    }

    pub fn row_count(&self) -> usize {
        if self.open {
            self.labels.len().min(8) + usize::from(self.signature.is_some())
        } else {
            usize::from(self.diagnostic.is_some())
        }
    }

    pub fn dismiss(&mut self) -> bool {
        let visible = self.open || self.diagnostic.is_some();
        self.open = false;
        self.diagnostic = None;
        visible
    }

    pub fn select(&mut self, back: bool) -> bool {
        if !self.open || self.labels.is_empty() {
            return false;
        }
        let count = self.labels.len();
        self.selected = if back {
            (self.selected + count - 1) % count
        } else {
            (self.selected + 1) % count
        };
        true
    }

    pub fn complete(
        &mut self,
        editor: &mut Editor,
        back: bool,
        localizer: &Localizer,
        locale: Locale,
    ) {
        if editor.is_composing() {
            return;
        }
        if !self.open {
            self.edited(editor, localizer, locale);
            self.open = !self.labels.is_empty() || self.signature.is_some();
        }
        if back {
            self.select(true);
            return;
        }
        let Some(item) = self
            .result
            .as_ref()
            .and_then(|r| r.items.get(self.selected))
        else {
            return;
        };
        if editor.apply_completion(item) {
            self.edited(editor, localizer, locale);
        }
    }

    pub fn ready(&mut self, editor: &Editor, localizer: &Localizer, locale: Locale) -> bool {
        if editor.text.trim().is_empty() || editor.is_composing() {
            return false;
        }
        self.edited(editor, localizer, locale);
        let result = self.result.as_ref().expect("current analysis");
        let Err(error) = validate_edit(&editor.text, result) else {
            return true;
        };
        let incomplete = matches!(
            result.state,
            minimium_commands::completion::AnalysisState::Incomplete
        );
        let notice = if incomplete {
            UiNotice::CompletionIncomplete
        } else {
            UiNotice::CompletionInvalid
        };
        let mut message = localizer.format(locale, &UiMessage::Notice(notice));
        if let Some(e) = &error {
            message.push_str(&format!(" [{}] {}", e.code.as_str(), e.reason));
        }
        let span = error.and_then(|e| e.span).unwrap_or(SourceSpan {
            start: editor.cursor,
            end: editor.cursor,
        });
        self.diagnostic = Some((message, span));
        self.open = false;
        false
    }
}
