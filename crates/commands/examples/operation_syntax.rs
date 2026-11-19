use minimium_commands::operation::*;
use minimium_commands::syntax::bind::{bind_line, ParsedLine};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let binding = DomainBinding {
        owner: "Settings",
        scope: "Client",
        context: "camera",
        permission: "Local",
        capability: None,
        source: "ClientHost",
        persistence: PersistenceKind::ClientConfig,
    };
    let field = binding.declaration(
        "client.camera.fov",
        OperationKind::Property,
        &[ParameterSpec::required("value", "FiniteF32").scalar(ScalarType::FiniteF32)],
        TypeRef("Fov"),
        Some("deg"),
        Effect::LocalPresentation,
    );
    let mut builder = CatalogBuilder::default();
    builder.register([field])?;
    let catalog = builder.finish();
    if let ParsedLine::Operation(prepared) = bind_line(&catalog, "client.camera.fov = 90")? {
        println!(
            "{}",
            prepared.explanation(catalog.resolve(&prepared.path).unwrap())
        );
    }
    use minimium_commands::admin::{AdminPayload, AdminRequestV1, Correlation};
    use minimium_commands::syntax::{canonical::AccessKind, typed::bind_operation};
    let request = AdminRequestV1::new(
        Correlation {
            nonce: 1,
            request_id: 1,
        },
        AdminPayload::Operation {
            operation_id: OperationId("client.camera.fov".into()),
            access: AccessKind::Read,
            arguments: Vec::new(),
            value: None,
        },
    );
    let decoded = AdminRequestV1::decode(&request.encode()?)?;
    let prepared = bind_operation(&catalog, &decoded.payload)?;
    println!("{} {}", decoded.correlation.request_id, prepared.path);
    Ok(())
}
