use std::process::Command;

fn main() {
    // Build-script cfg evaluates for the *host*, so gate on the target via
    // CARGO_CFG_TARGET_OS instead. On host `cargo test` this is a no-op.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("espidf") {
        embuild::espidf::sysenv::output();
    }

    let git_hash = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    let dirty = Command::new("git")
        .args(["status", "--porcelain"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| !o.stdout.is_empty())
        .unwrap_or(false);

    let suffix = if dirty { "-dirty" } else { "" };
    println!("cargo:rustc-env=KUROKKU_GIT_HASH={}{}", git_hash, suffix);

    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=.git/index");

    // Keep partitions.csv in sync with esp-idf-sys's cmake build dirs.
    // The cmake build resolves CONFIG_PARTITION_TABLE_CUSTOM_FILENAME relative
    // to its own source dir (out/), not the project root, so we mirror it there.
    // This runs after esp-idf-sys's cmake build completes, ensuring the file is
    // present for the next incremental build. The Justfile handles bootstrap
    // (first build) by retrying after the initial cmake failure creates out/.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("espidf") {
        let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
        let out_dir = std::env::var("OUT_DIR").unwrap();
        let src = std::path::Path::new(&manifest_dir).join("partitions.csv");
        // OUT_DIR = .../build/<pkg-hash>/out — navigate up two levels to build/
        if let Some(build_dir) = std::path::Path::new(&out_dir).parent().and_then(|p| p.parent()) {
            if let Ok(entries) = std::fs::read_dir(build_dir) {
                for entry in entries.flatten() {
                    if entry.file_name().to_string_lossy().starts_with("esp-idf-sys-") {
                        let dest = entry.path().join("out").join("partitions.csv");
                        if dest.parent().map(|p| p.is_dir()).unwrap_or(false) {
                            let _ = std::fs::copy(&src, &dest);
                        }
                    }
                }
            }
        }
        println!("cargo:rerun-if-changed=partitions.csv");
    }
}
