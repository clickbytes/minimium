//! Offline static asset validation, without a device, session, or game process.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args_os()
        .nth(1)
        .ok_or("usage: validate_materials PACKAGE_DIRECTORY")?;
    for compressed in [true, false] {
        let package = minimium_asset_io::materials::MaterialPackage::open(
            std::path::Path::new(&path),
            compressed,
        )?;
        let variant = &package.manifest.variants[package.variant];
        println!(
            "{}: edge={} compressed={} files=3 verified",
            package.manifest_hash, variant.edge, variant.compressed
        );
    }
    Ok(())
}
