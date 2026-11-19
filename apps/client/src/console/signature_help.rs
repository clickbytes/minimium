use minimium_commands::completion::SignatureHelp;

pub(super) fn declaration(s: &SignatureHelp) -> String {
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
    format!("({args}{})", if s.truncated { "…" } else { "" })
}

pub(super) fn active(s: &SignatureHelp) -> String {
    let focus = s.arguments.get(s.active).map_or(String::new(), |p| {
        format!("[{}: {}] ", p.name, p.value_type.0)
    });
    format!(
        "{focus}{}{} -> {}",
        s.operation.0,
        declaration(s),
        s.result.0
    )
    .chars()
    .take(192)
    .collect()
}
