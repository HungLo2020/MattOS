fn isolate_cargo_build_mirror(source: &Path) -> Result<()> {
    let _lock = ConsumerMirrorLock::acquire(&source_lock_repo_root(source)?, source)?;
    let manifest = source.join("Cargo.toml");
    if !manifest.is_file() {
        return Ok(());
    }
    let body = fs::read_to_string(&manifest)?;
    if !body.lines().any(|line| line.trim() == "[workspace]") {
        let mut file = fs::OpenOptions::new().append(true).open(&manifest)?;
        file.write_all(b"\n# MattOS output-owned build-mirror isolation.\n[workspace]\n")?;
    }
    Ok(())
}
