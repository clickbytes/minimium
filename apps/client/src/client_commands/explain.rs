use minimium_commands::completion::{analyze_partial, AnalysisState, CompletionResult};
use minimium_commands::CommandError;
use std::ops::Range;

pub(crate) fn analyze(
    input: &str,
    cursor: usize,
    selection: Range<usize>,
    revision: u64,
) -> CompletionResult {
    analyze_partial(
        input,
        cursor,
        selection,
        revision,
        super::catalog::catalog(),
    )
}

pub(super) fn describe(input: &str) -> Result<String, CommandError> {
    let result = analyze(input, input.len(), input.len()..input.len(), 0);
    match result.state {
        AnalysisState::Complete {
            prepared,
            explanation,
        } => {
            if prepared.path != "ui.console.explain" {
                super::bindings::lower(
                    &prepared,
                    super::catalog::catalog()
                        .resolve(&prepared.path)
                        .expect("bound operation"),
                )?;
            }
            Ok(explanation.to_string())
        }
        AnalysisState::Invalid(error) => Err(error),
        AnalysisState::Namespace(path) => Ok(format!("namespace {path}")),
        AnalysisState::HostQuery => Ok(format!("catalog query {:?}", super::text::legacy(input)?)),
        AnalysisState::Incomplete => Ok(format!(
            "incomplete {} {}",
            result.signature.map_or_else(String::new, |s| format!(
                "{} argument {}",
                s.operation.0,
                s.active + 1
            )),
            result
                .items
                .iter()
                .map(|i| i.label.as_str())
                .collect::<Vec<_>>()
                .join(" ")
        )),
    }
}
