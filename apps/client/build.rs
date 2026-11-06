use std::{env, path::PathBuf, process::Command};

fn main() {
    if env::var_os("CARGO_FEATURE_DESKTOP").is_none() {
        return;
    }
    let root = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap()).join("../..");
    let output = root.join("target/materials/sample-v1");
    let script = root.join("tools/bake_materials.py");
    println!("cargo:rerun-if-changed={}", script.display());
    println!(
        "cargo:rerun-if-changed={}",
        root.join("games/sample/materials/recipe.py").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        root.join("games/sample/materials/fields.py").display()
    );
    println!("cargo:rerun-if-env-changed=MINIMIUM_ASSET_PYTHON");
    let python = env::var_os("MINIMIUM_ASSET_PYTHON").unwrap_or_else(|| "python".into());
    let status = Command::new(python)
        .arg(script)
        .arg(&output)
        .status()
        .expect("material cooker needs Python 3 with numpy");
    assert!(status.success(), "terrain material cooking failed");
    println!(
        "cargo:rustc-env=MINIMIUM_BUILT_MATERIALS={}",
        output.display()
    );
}
