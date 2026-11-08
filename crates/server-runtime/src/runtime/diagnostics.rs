pub(super) fn emit_system_effect_diagnostic(
    logger: &minimium_diagnostics::DomainLogger,
    action: &str,
    reason: &str,
) {
    if let Ok(code) = minimium_diagnostics::EventCode::new("gameplay.effect_rejected") {
        logger.emit(
            logger
                .event(minimium_diagnostics::Level::Warn, code)
                .field("action", action)
                .field("reason", reason),
        );
    }
}
