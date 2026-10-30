use super::state::Console;
use super::theme::{ECHO_TEXT, MAX_LINES};

pub(super) fn take_input(console: &mut Console) -> String {
    let input = console.editor.clear();
    console.history_cursor = None;
    console.history_draft.clear();
    console.suggestions.edited(&console.editor);
    if !input.trim().is_empty() {
        console.scroll_to_latest();
        if console.history.last() != Some(&input) {
            if console.history.len() == MAX_LINES {
                console.history.remove(0);
            }
            console.history.push(input.clone());
        }
        console.push_colored(&format!("> {input}"), ECHO_TEXT);
    }
    input
}

pub(super) fn history_up(console: &mut Console) {
    if console.history.is_empty() {
        return;
    }
    if console.history_cursor.is_none() {
        console.history_draft = console.editor.text.clone();
    }
    let index = console
        .history_cursor
        .unwrap_or(console.history.len())
        .saturating_sub(1);
    console.history_cursor = Some(index);
    console.editor.set_text(&console.history[index]);
    console.suggestions.edited(&console.editor);
}

pub(super) fn history_down(console: &mut Console) {
    let Some(index) = console.history_cursor else {
        return;
    };
    if index + 1 >= console.history.len() {
        console.history_cursor = None;
        console.editor.set_text(&console.history_draft);
    } else {
        console.history_cursor = Some(index + 1);
        console.editor.set_text(&console.history[index + 1]);
    }
    console.suggestions.edited(&console.editor);
}
