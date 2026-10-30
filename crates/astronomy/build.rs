use std::{env, path::PathBuf};

fn main() {
    let manifest = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("manifest dir"));
    let bridge_dir = manifest.join("ffi");
    let vendor_dir = manifest.join("../../third_party/astronomy-engine/source/c");
    let bridge = bridge_dir.join("bridge.c");
    let upstream = vendor_dir.join("astronomy.c");
    println!("cargo:rerun-if-changed={}", bridge.display());
    println!(
        "cargo:rerun-if-changed={}",
        bridge_dir.join("bridge.h").display()
    );
    println!("cargo:rerun-if-changed={}", upstream.display());
    println!(
        "cargo:rerun-if-changed={}",
        vendor_dir.join("astronomy.h").display()
    );

    cc::Build::new()
        .file(bridge)
        .file(upstream)
        .include(bridge_dir)
        .include(vendor_dir)
        .define("ASTRONOMY_ENGINE_NO_CURRENT_TIME", None)
        .warnings(true)
        .compile("minimium_astronomy_engine");

    if env::var("CARGO_CFG_TARGET_FAMILY").as_deref() == Ok("unix") {
        println!("cargo:rustc-link-lib=m");
    }
}
