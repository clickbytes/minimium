use crate::CommandToken;
use std::ops::Range;

pub(super) fn word(tokens: &[CommandToken], cursor: usize) -> Range<usize> {
    tokens
        .iter()
        .find(|t| t.span.start <= cursor && cursor <= t.span.end)
        .map_or(cursor..cursor, |t| t.span.start..t.span.end)
}

pub(super) fn replacement_range(
    line: &str,
    cursor: usize,
    selection: &Range<usize>,
    token: Range<usize>,
    path: bool,
) -> Range<usize> {
    if !selection.is_empty() {
        return selection.clone();
    }
    if !path {
        return token;
    }
    let start = line[token.start..cursor]
        .rfind('.')
        .map_or(token.start, |i| token.start + i + 1);
    let end = line[cursor..token.end]
        .find('.')
        .map_or(token.end, |i| cursor + i);
    start..end
}
