pub(crate) struct ScriptError {
    pub(crate) code: &'static str,
    pub(crate) reason: String,
}

impl ScriptError {
    pub(crate) fn new(code: &'static str, reason: impl Into<String>) -> Self {
        Self {
            code,
            reason: reason.into(),
        }
    }
}

pub(crate) fn format_status(
    value: &minimium_commands::operation::CommandValue,
    locale: minimium_localization::Locale,
) -> String {
    use minimium_commands::operation::CommandValue as V;
    let V::Record(fields) = value else {
        return value.encode();
    };
    let get = |key: &str| {
        fields
            .get(key)
            .map_or_else(|| "unavailable".into(), V::encode)
    };
    let (run, phase, calls, mutations, output, error, operation) = match locale {
        minimium_localization::Locale::Polish => (
            "uruchomienie",
            "faza",
            "wywołania",
            "zmiany",
            "wyjście",
            "błąd",
            "operacja",
        ),
        minimium_localization::Locale::English => (
            "run",
            "phase",
            "calls",
            "mutations",
            "output",
            "error",
            "operation",
        ),
    };
    let mut lines = vec![format!(
        "{run}={} {phase}={} {calls}={} {mutations}={}",
        get("run_id"),
        get("phase"),
        get("calls"),
        get("mutations")
    )];
    if let Some(V::Record(metrics)) = fields.get("metrics") {
        let read = |key: &str| {
            metrics
                .get(key)
                .map_or_else(|| "unavailable".into(), V::encode)
        };
        lines.push(format!(
            "cache_hit={} source_bytes={} result_bytes={} termination={} current_phase={}",
            read("cache_hit"),
            read("source_bytes"),
            read("result_bytes"),
            read("termination_code"),
            get("current_phase")
        ));
        for key in [
            "load",
            "worker_start",
            "cache_lookup",
            "adapter_setup",
            "compile",
            "evaluate",
            "wait_for_owner",
            "format",
        ] {
            if let Some(V::Record(phase)) = metrics.get(key) {
                let field = |name: &str| {
                    phase
                        .get(name)
                        .map_or_else(|| "unavailable".into(), V::encode)
                };
                lines.push(format!(
                    "{key}: {} {} us={}",
                    field("state"),
                    field("source"),
                    field("duration_us")
                ));
            }
        }
    }
    if let Some(budget) = fields.get("budget") {
        lines.push(format!("budget={}", budget.encode()));
    }
    if let Some(V::Page { rows, .. }) = fields.get("operations") {
        for row in rows {
            if let V::Record(call) = row {
                let item = |key: &str| call.get(key).map_or_else(|| "none".into(), V::encode);
                lines.push(format!(
                    "{operation}={} stage={} request={} nonce={} error={}",
                    item("path"),
                    item("stage"),
                    item("request_id"),
                    item("nonce"),
                    item("error_code")
                ));
            }
        }
    }
    if let Some(V::Text(text)) = fields.get("output_preview") {
        if !text.is_empty() {
            lines.push(format!("{output}: {text}"));
        }
    }
    if let Some(V::Text(text)) = fields.get("error") {
        lines.push(format!("{error}: {text}"));
    }
    lines.join("\n")
}
