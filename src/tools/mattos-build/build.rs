use std::path::PathBuf;
use std::process::Command;

fn main() {
    let manifest_dir = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let repo_root = manifest_dir
        .ancestors()
        .nth(3)
        .expect("mattos-build must live under src/tools/mattos-build");
    let sources = repo_root.join("upstream/sources.toml");
    let generator = repo_root.join("DevUtils/generate_source_overrides.py");

    println!("cargo:rerun-if-changed={}", sources.display());
    println!("cargo:rerun-if-changed={}", generator.display());

    let status = Command::new("python3")
        .arg(&generator)
        .current_dir(repo_root)
        .status()
        .expect("failed to run MattOS source-ownership generator");
    assert!(
        status.success(),
        "MattOS source-ownership generation failed; external copies of owned sources are not allowed"
    );
}
