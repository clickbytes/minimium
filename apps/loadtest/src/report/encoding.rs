pub(super) fn json_string(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len() + 2);
    escaped.push('"');
    for character in value.chars() {
        match character {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            character if character <= '\u{1f}' => {
                escaped.push_str(&format!("\\u{:04x}", character as u32));
            }
            character => escaped.push(character),
        }
    }
    escaped.push('"');
    escaped
}

pub(super) fn json_number(value: Option<f64>) -> String {
    value.map_or_else(|| "null".to_owned(), |number| format!("{number:.3}"))
}

pub(super) fn display_number(value: Option<f64>) -> String {
    value.map_or_else(|| "n/a".to_owned(), |number| format!("{number:.3}"))
}
