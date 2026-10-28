use super::state::Console;
use super::theme::{ECHO_TEXT, MAX_INPUT, MAX_LINES};
use crate::client_commands::completion_candidates;

pub(super) fn take_input(console: &mut Console) -> String {
    let input = std::mem::take(&mut console.input);
    console.input = String::with_capacity(MAX_INPUT);
    console.history_cursor = None;
    if !input.trim().is_empty() {
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
    let index = console
        .history_cursor
        .unwrap_or(console.history.len())
        .saturating_sub(1);
    console.history_cursor = Some(index);
    console.input.clear();
    console.input.push_str(&console.history[index]);
}

pub(super) fn history_down(console: &mut Console) {
    let Some(index) = console.history_cursor else {
        return;
    };
    if index + 1 >= console.history.len() {
        console.history_cursor = None;
        console.input.clear();
    } else {
        console.history_cursor = Some(index + 1);
        console.input.clear();
        console.input.push_str(&console.history[index + 1]);
    }
}

pub(super) fn complete_input(console: &mut Console) {
    let input = console.input.trim();
    let matches: Vec<_> = completion_candidates()
        .filter(|command| command.starts_with(input))
        .collect();
    if matches.len() == 1 {
        console.input.clear();
        console.input.push_str(matches[0]);
        console.input.push(' ');
    } else if matches.len() > 1 {
        let mut suggestions = String::new();
        for command in &matches {
            if !suggestions.is_empty() {
                suggestions.push_str("  ");
            }
            suggestions.push_str(command);
        }
        console.push_line(suggestions);
    }
}
