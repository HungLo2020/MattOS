fn load_source_selection_policy(
    repo_root: &Path,
    comp: &ComponentDef,
) -> Result<(Option<SourceSelectionPolicy>, String, String)> {
    let (policy_name, expected_sha256) =
        component_source_selection_metadata(repo_root, &comp.name)?;
    if policy_name == "none" {
        if expected_sha256 != "none" {
            bail!(
                "{} records a source-selection digest without a policy",
                comp.name
            );
        }
        return Ok((None, policy_name, expected_sha256));
    }
    let policy_path = resolve_component_destination(repo_root, &policy_name)?;
    let payload = fs::read(&policy_path).with_context(|| {
        format!(
            "failed to read source-selection policy: {}",
            policy_path.display()
        )
    })?;
    let actual_sha256 = format!("{:x}", Sha256Hasher::digest(&payload));
    if actual_sha256 != expected_sha256 {
        bail!(
            "{} source-selection policy checksum mismatch: expected {}, got {}",
            comp.name,
            expected_sha256,
            actual_sha256
        );
    }
    let policy: SourceSelectionPolicy = toml::from_str(
        std::str::from_utf8(&payload).context("source-selection policy is not UTF-8")?,
    )
    .with_context(|| format!("failed to parse {}", policy_path.display()))?;
    if policy.schema_version != 1
        || policy.component != comp.name
        || policy.scope != "arch"
        || comp.revision.as_deref() != Some(policy.upstream_commit.as_str())
    {
        bail!(
            "{} source-selection policy metadata does not match sources.toml",
            comp.name
        );
    }
    Ok((Some(policy), policy_name, expected_sha256))
}

fn component_source_selection_metadata(
    repo_root: &Path,
    component_name: &str,
) -> Result<(String, String)> {
    let source_path = repo_root.join("upstream/sources.toml");
    if !source_path.is_file() {
        return Ok(("none".to_string(), "none".to_string()));
    }
    let source_text = fs::read_to_string(&source_path)
        .with_context(|| format!("failed to read {}", source_path.display()))?;
    let document: toml::Value = toml::from_str(&source_text)
        .with_context(|| format!("failed to parse {}", source_path.display()))?;
    let component = document
        .get("component")
        .and_then(toml::Value::as_array)
        .and_then(|components| {
            components.iter().find(|value| {
                value.get("name").and_then(toml::Value::as_str) == Some(component_name)
            })
        });
    let field = |name: &str| {
        component
            .and_then(|value| value.get(name))
            .and_then(toml::Value::as_str)
            .unwrap_or("none")
            .to_string()
    };
    Ok((
        field("source_selection_policy"),
        field("source_selection_policy_sha256"),
    ))
}

fn apply_source_selection(
    destination: &Path,
    source_selection: Option<&SourceSelectionPolicy>,
) -> Result<()> {
    let Some(policy) = source_selection else {
        return Ok(());
    };
    let arch_root = destination.join("arch");
    if !arch_root.is_dir() {
        bail!("source-selection policy requires {}", arch_root.display());
    }
    prune_source_selection_tree(destination, &arch_root, policy)?;
    Ok(())
}

fn prune_source_selection_tree(
    destination: &Path,
    directory: &Path,
    policy: &SourceSelectionPolicy,
) -> Result<()> {
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.is_dir() && !metadata.file_type().is_symlink() {
            prune_source_selection_tree(destination, &path, policy)?;
            if fs::read_dir(&path)?.next().is_none() {
                fs::remove_dir(&path)?;
            }
            continue;
        }
        let relative = path
            .strip_prefix(destination)
            .expect("source-selection path is inside destination")
            .to_string_lossy();
        if !policy.retains(&relative) {
            fs::remove_file(path)?;
        }
    }
    Ok(())
}
