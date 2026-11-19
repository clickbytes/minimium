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
