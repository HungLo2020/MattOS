fn component_provenance_policy(
    repo_root: &Path,
    component_name: &str,
) -> Result<(String, String, String, String, String, String)> {
    let source_path = repo_root.join("upstream/sources.toml");
    if !source_path.is_file() {
        return Ok((
            "none".to_string(),
            "none".to_string(),
            "none".to_string(),
            "none".to_string(),
            "none".to_string(),
            "none".to_string(),
        ));
    }
    let source_text = fs::read_to_string(&source_path)
        .with_context(|| format!("failed to read {}", source_path.display()))?;
    let document: toml::Value = toml::from_str(&source_text)
        .with_context(|| format!("failed to parse {}", source_path.display()))?;
    let components = document
        .get("component")
        .and_then(toml::Value::as_array)
        .ok_or_else(|| anyhow!("sources metadata has no component array"))?;
    let Some(component) = components
        .iter()
        .find(|value| value.get("name").and_then(toml::Value::as_str) == Some(component_name))
    else {
        return Ok((
            "none".to_string(),
            "none".to_string(),
            "none".to_string(),
            "none".to_string(),
            "none".to_string(),
            "none".to_string(),
        ));
    };
    let field = |name: &str| {
        component
            .get(name)
            .and_then(toml::Value::as_str)
            .unwrap_or("none")
            .to_string()
    };
    Ok((
        field("intentional_omission_policy"),
        field("gitlink_policy"),
        field("patch_manifest"),
        field("patch_manifest_sha256"),
        field("lfs_policy"),
        field("lfs_policy_sha256"),
    ))
}
