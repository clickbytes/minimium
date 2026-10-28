use std::{env, process::Command};

fn main() {
    println!("cargo:rerun-if-env-changed=RUSTC");
    let rustc = env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
    let version = Command::new(rustc)
        .arg("--version")
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "n/a".to_owned());
    let profile = env::var("PROFILE").unwrap_or_else(|_| "unknown".to_owned());
    println!("cargo:rustc-env=MINIMIUM_RUSTC_VERSION={version}");
    println!("cargo:rustc-env=MINIMIUM_BUILD_PROFILE={profile}");
}
