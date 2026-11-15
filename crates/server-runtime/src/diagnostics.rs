use minimium_diagnostics::{DomainLogger, EventCode, Level};

pub(crate) fn emit(
    logger: &DomainLogger,
    level: Level,
    code: &'static str,
    fields: &[(&str, String)],
) {
    let Ok(code) = EventCode::new(code) else {
        return;
    };
    let mut event = logger.event(level, code);
    for (key, value) in fields {
        event = event.field(key, value);
    }
    logger.emit(event);
}
