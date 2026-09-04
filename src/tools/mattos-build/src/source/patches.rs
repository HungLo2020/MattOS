/// Applies checksummed MattOS patches only after authoritative source has been
/// copied to an output-owned mirror. Vendored source trees remain byte-for-byte
/// equal to their pinned upstream trees.
fn apply_component_patches(
    repo_root: &Path,
    component_name: &str,
    source_mirror: &Path,
) -> Result<()> {
    let output_root = repo_root.join("out");
    if !source_mirror.starts_with(&output_root) {
        bail!(
            "refusing to patch non-output source tree {}",
            source_mirror.display()
        );
    }
    let _lock = ConsumerMirrorLock::acquire(repo_root, source_mirror)?;
    let mirror_relative = source_mirror.strip_prefix(repo_root).with_context(|| {
        format!(
            "output mirror is outside repository: {}",
            source_mirror.display()
        )
    })?;
    let directory_arg = format!("--directory={}", mirror_relative.display());
    let state = read_sync_state(repo_root, component_name)?
        .ok_or_else(|| anyhow!("missing provenance state for {component_name}"))?;
    if state.patch_manifest == "none" {
        return Ok(());
    }
    let manifest_relative = validated_repo_relative_path(&state.patch_manifest)?;
    let manifest_path = repo_root.join(manifest_relative);
    let manifest_sha256 = performance::sha256_file(&manifest_path)?;
    if manifest_sha256 != state.patch_manifest_sha256 {
        bail!(
            "patch manifest checksum mismatch for {}: expected {}, got {}",
            manifest_path.display(),
            state.patch_manifest_sha256,
            manifest_sha256
        );
    }
    let body = fs::read_to_string(&manifest_path)
        .with_context(|| format!("failed to read patch manifest {}", manifest_path.display()))?;
    let manifest: ComponentPatchManifest = toml::from_str(&body)
        .with_context(|| format!("failed to parse patch manifest {}", manifest_path.display()))?;
    if manifest.component != component_name {
        bail!("patch manifest component does not match {component_name}");
    }
    if manifest.application != "output-mirror-only" {
        bail!("patch manifest for {component_name} is not output-mirror-only");
    }
    for record in manifest.patch {
        let patch_relative = validated_repo_relative_path(&record.path)?;
        let patch_path = repo_root.join(patch_relative);
        let actual = performance::sha256_file(&patch_path)?;
        if actual != record.sha256 {
            bail!(
                "patch checksum mismatch for {}: expected {}, got {}",
                patch_path.display(),
                record.sha256,
                actual
            );
        }
        let patch_text = patch_path
            .to_str()
            .ok_or_else(|| anyhow!("patch path is not valid UTF-8: {}", patch_path.display()))?;
        run_cmd(
            repo_root,
            "git",
            &[
                "apply",
                "--check",
                "--unidiff-zero",
                "--whitespace=error-all",
                directory_arg.as_str(),
                patch_text,
            ],
        )?;
        run_cmd(
            repo_root,
            "git",
            &[
                "apply",
                "--unidiff-zero",
                "--whitespace=error-all",
                directory_arg.as_str(),
                patch_text,
            ],
        )?;
    }
    Ok(())
}
