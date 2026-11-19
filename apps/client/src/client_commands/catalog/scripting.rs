use minimium_commands::operation::{
    CatalogEntry, DomainBinding, Effect, OperationKind, ParameterSpec, PersistenceKind, ScalarType,
    TypeRef,
};

pub(super) fn entry(name: &str) -> Option<CatalogEntry> {
    let binding = DomainBinding {
        owner: "ClientScriptHost",
        scope: "Client",
        context: "local host job",
        permission: "Local",
        capability: Some("ConsoleScripting"),
        source: "ClientHost",
        persistence: PersistenceKind::Transient,
    };
    Some(match name {
        "client.scripting.run_file" => binding.declaration(
            name,
            OperationKind::Method,
            &[ParameterSpec::required("path", "LocalScriptPath").scalar(ScalarType::Text)],
            TypeRef::domain("ScriptRunReceipt"),
            Some("UTF-8 bytes"),
            Effect::LocalJob,
        ),
        "client.scripting.status" => binding.declaration(
            name,
            OperationKind::Method,
            &[ParameterSpec::required("run_id", "ScriptRunId").scalar(ScalarType::NonzeroU64)],
            TypeRef::domain("ScriptStatus"),
            None,
            Effect::Query,
        ),
        "client.scripting.cancel" => binding.declaration(
            name,
            OperationKind::Method,
            &[ParameterSpec::required("run_id", "ScriptRunId").scalar(ScalarType::NonzeroU64)],
            TypeRef::domain("CancelReceipt"),
            None,
            Effect::LocalJob,
        ),
        _ => return None,
    })
}
