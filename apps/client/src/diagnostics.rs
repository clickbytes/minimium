use minimium_diagnostics::{DomainLogger, EventCode, Level};

pub(crate) fn failure(logger: &DomainLogger, code: &'static str, reason: &str) {
    if let Ok(code) = EventCode::new(code) {
        logger.emit(logger.event(Level::Error, code).field("reason", reason));
    }
}
