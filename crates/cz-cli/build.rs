//! Records the compiler that built `cz` as `CZ_RUSTC_VERSION`, for the
//! `RUN.json` manifest of `cz lab run` (ADR-0015 D2: machine, commit,
//! compiler and binary of a run are part of the record).

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=RUSTC");
    let rustc = std::env::var("RUSTC").unwrap_or_else(|_| "rustc".to_string());
    let version = std::process::Command::new(rustc)
        .arg("--version")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();
    println!("cargo:rustc-env=CZ_RUSTC_VERSION={version}");
}
