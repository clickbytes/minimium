use minimium_commands::operation::*;

fn main() -> Result<(), CatalogError> {
    let binding = DomainBinding {
        owner: "ExampleCamera",
        scope: "Client",
        context: "local camera",
        permission: "Local",
        capability: None,
        source: "ClientHost",
        persistence: PersistenceKind::ClientConfig,
    };
    let mut field = binding.declaration(
        "client.camera.fov",
        OperationKind::Property,
        &[ParameterSpec::required("value", "degrees")],
        TypeRef::domain("camera.fov/v1"),
        Some("deg"),
        Effect::LocalPresentation,
    );
    field.aliases.push(AliasSpec {
        name: "fov",
        usage: "fov DEGREES",
        arguments: &[],
    });
    let mut builder = CatalogBuilder::default();
    builder.register([field])?;
    let catalog = builder.finish();
    if let Some(entry) = catalog.resolve("fov") {
        println!("{}", entry.describe());
    }
    Ok(())
}
