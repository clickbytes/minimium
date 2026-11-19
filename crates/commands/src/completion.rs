//! Read-only source analysis shared by terminal explanation and the editor.
mod ranges;
mod signature;
mod suggest;
use crate::operation::{Access, OperationCatalog, OperationId, TypeRef};
use crate::syntax::bind::{
    bind_line, legacy_prefix, OperationExplanation, ParsedLine, PreparedOperation,
};
use crate::{CommandError, CommandErrorCode as Code, CommandToken};
pub use signature::SignatureHelp;
use std::ops::Range;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ItemKind {
    Namespace,
    Property,
    Method,
    Argument,
}

#[derive(Clone, Debug)]
pub struct CompletionItem {
    pub replace_range: Range<usize>,
    pub insert_text: String,
    /// Byte offset relative to the beginning of the edit, after application.
    pub caret_offset: usize,
    pub buffer_revision: u64,
    pub kind: ItemKind,
    pub label: String,
    pub value_type: Option<TypeRef>,
    pub writable: bool,
    pub signature: Option<SignatureHelp>,
    pub unit: Option<&'static str>,
    pub access: Option<Access>,
    pub description: String,
    pub operation: Option<OperationId>,
}

#[derive(Clone, Debug)]
pub enum AnalysisState {
    Complete {
        prepared: PreparedOperation,
        explanation: OperationExplanation,
    },
    Namespace(String),
    HostQuery,
    Incomplete,
    Invalid(CommandError),
}

#[derive(Clone, Debug)]
pub struct CompletionResult {
    pub state: AnalysisState,
    pub diagnostic: Option<CommandError>,
    pub tokens: Vec<CommandToken>,
    pub items: Vec<CompletionItem>,
    pub signature: Option<SignatureHelp>,
    pub truncated: bool,
    pub buffer_revision: u64,
    pub catalog_revision: u64,
}

pub const MAX_ITEMS: usize = 64;

pub fn analyze_partial(
    line: &str,
    cursor: usize,
    selection: Range<usize>,
    buffer_revision: u64,
    catalog: &OperationCatalog,
) -> CompletionResult {
    let mut result = CompletionResult {
        diagnostic: None,
        state: AnalysisState::Incomplete,
        tokens: Vec::new(),
        items: Vec::new(),
        signature: None,
        truncated: false,
        buffer_revision,
        catalog_revision: catalog.fingerprint(),
    };
    if line.len() > crate::MAX_INPUT_BYTES {
        result.state = AnalysisState::Invalid(crate::input_too_long(line.len()));
        return result;
    }
    if !line.is_char_boundary(cursor) || line.get(selection.clone()).is_none() {
        result.state = AnalysisState::Invalid(CommandError::new(
            Code::InvalidArguments,
            Some("input"),
            "invalid input span or byte budget",
            None::<String>,
        ));
        return result;
    }
    let alias = legacy_prefix(catalog, line);
    let words = crate::syntax::lex_words(line, true).unwrap_or_default();
    let verb = words.first().map(|w| w.value.as_str()).unwrap_or("");
    let explicit = matches!(verb, "get" | "set" | "help" | "list" | "reset")
        && words.first().is_some_and(|t| t.span.end < line.len());
    let canonical = !explicit && alias.is_none() && crate::syntax::bind::canonical_input(line);
    result.tokens = if canonical {
        crate::syntax::canonical::edit_tokens(line)
    } else {
        words.clone()
    };
    suggest::populate(
        &mut result,
        line,
        cursor,
        selection,
        catalog,
        explicit,
        canonical,
        alias,
    );
    result.state = match bind_line(catalog, line) {
        Ok(ParsedLine::Operation(op)) => AnalysisState::Complete {
            explanation: op.explanation(catalog.resolve(&op.path).expect("bound operation")),
            prepared: op,
        },
        Ok(ParsedLine::Namespace(path)) => AnalysisState::Namespace(path),
        Ok(ParsedLine::Legacy) if matches!(verb, "help" | "list") => AnalysisState::HostQuery,
        Ok(ParsedLine::Legacy) if !line.trim().is_empty() && result.items.is_empty() => {
            AnalysisState::Invalid(CommandError::new(
                Code::UnknownCommand,
                Some(verb),
                "unknown command",
                None::<String>,
            ))
        }
        Ok(ParsedLine::Legacy) => AnalysisState::Incomplete,
        Err(e)
            if e.code == Code::MissingArgument
                || e.reason.contains("unclosed quoted")
                || e.span.is_some_and(|s| s.start == line.len())
                || (matches!(
                    e.code,
                    Code::UnknownKey | Code::UnknownCommand | Code::UnknownProperty
                ) && !result.items.is_empty()) =>
        {
            result.diagnostic = Some(e);
            AnalysisState::Incomplete
        }
        Err(e) => AnalysisState::Invalid(e),
    };
    result
}
