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

    // Track only Cargo manifests rather than the entire multi-gigabyte source
    // tree. This makes ownership regeneration automatic when dependency edges
    // change without turning Cargo's build-script freshness check into a full
    // Linux/LLVM/Rust filesystem walk.
    let manifests = Command::new("git")
        .args(["ls-files", "-z", "--", ":(glob)src/**/Cargo.toml"])
        .current_dir(repo_root)
        .output()
        .expect("failed to enumerate MattOS Cargo manifests");
    assert!(manifests.status.success(), "git ls-files failed while preparing source ownership");
    for raw in manifests.stdout.split(|byte| *byte == 0).filter(|part| !part.is_empty()) {
        let relative = String::from_utf8_lossy(raw);
        println!("cargo:rerun-if-changed={}", repo_root.join(relative.as_ref()).display());
    }

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
