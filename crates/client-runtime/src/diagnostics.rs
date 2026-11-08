use minimium_diagnostics::{DiagnosticField, DomainLogger, EventCode, Level};

pub(crate) fn emit(
    logger: Option<&DomainLogger>,
    level: Level,
    code: &'static str,
    fields: impl IntoIterator<Item = DiagnosticField>,
) {
    let (Some(logger), Ok(code)) = (logger, EventCode::new(code)) else {
        return;
    };
    let mut event = logger.event(level, code);
    for field in fields {
        event = event.field(&field.key, &field.value);
    }
    logger.emit(event);
}

pub(crate) fn field(key: &str, value: impl ToString) -> DiagnosticField {
    DiagnosticField {
        key: key.to_owned(),
        value: value.to_string(),
    }
}
