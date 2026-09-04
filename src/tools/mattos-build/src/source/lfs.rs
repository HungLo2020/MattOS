fn load_lfs_hydration_policy(
    repo_root: &Path,
    comp: &ComponentDef,
    policy_name: &str,
    expected_policy_sha256: &str,
) -> Result<Option<LfsHydrationPolicy>> {
    if policy_name == "none" {
        if expected_policy_sha256 != "none" {
            bail!("{} has an LFS policy checksum but no policy", comp.name);
        }
        return Ok(None);
    }
    if expected_policy_sha256 == "none" {
        bail!("{} LFS policy has no pinned SHA-256", comp.name);
    }
    let path = resolve_component_destination(repo_root, policy_name)?;
    let actual_policy_sha256 = performance::sha256_file(&path)?;
    if actual_policy_sha256 != expected_policy_sha256 {
        bail!(
            "{} LFS policy checksum mismatch: expected {}, got {}",
            comp.name,
            expected_policy_sha256,
            actual_policy_sha256
        );
    }
    let policy: LfsHydrationPolicy = toml::from_str(
        &fs::read_to_string(&path)
            .with_context(|| format!("failed to read LFS policy {}", path.display()))?,
    )
    .with_context(|| format!("failed to parse LFS policy {}", path.display()))?;
    let revision = comp
        .revision
        .as_deref()
        .ok_or_else(|| anyhow!("{} LFS hydration requires an exact revision", comp.name))?;
    if policy.schema_version != 1
        || policy.component != comp.name
        || policy.upstream_commit != revision
        || !policy.source.contains("{path}")
    {
        bail!(
            "{} LFS policy identity is inconsistent with sources.toml",
            comp.name
        );
    }
    let mut paths = BTreeSet::new();
    for object in &policy.object {
        resolve_component_destination(Path::new("/"), &object.path)
            .with_context(|| format!("invalid LFS object path: {}", object.path))?;
        if !paths.insert(&object.path) {
            bail!("duplicate LFS object path: {}", object.path);
        }
        if object.sha256.len() != 64 || !object.sha256.bytes().all(|b| b.is_ascii_hexdigit()) {
            bail!("invalid LFS SHA-256 for {}", object.path);
        }
    }
    Ok(Some(policy))
}

fn lfs_pointer(object: &LfsHydrationObject) -> String {
    format!(
        "version https://git-lfs.github.com/spec/v1\noid sha256:{}\nsize {}\n",
        object.sha256, object.size
    )
}

fn restore_lfs_pointers_for_merge(
    merge_tree: &Path,
    policy: Option<&LfsHydrationPolicy>,
) -> Result<()> {
    let Some(policy) = policy else {
        return Ok(());
    };
    for object in &policy.object {
        let object_name = format!("HEAD:{}", object.path);
        if run_cmd_status(merge_tree, "git", &["cat-file", "-e", &object_name])?.success() {
            run_cmd(merge_tree, "git", &["checkout", "HEAD", "--", &object.path])?;
        } else {
            remove_path_if_exists(&merge_tree.join(&object.path))?;
        }
    }
    Ok(())
}

fn hydrate_lfs_objects(
    repo_root: &Path,
    comp: &ComponentDef,
    destination: &Path,
    policy: Option<&LfsHydrationPolicy>,
) -> Result<()> {
    let Some(policy) = policy else {
        return Ok(());
    };
    let cache = repo_root
        .join("upstream/.tmp")
        .join(format!("{}-lfs", comp.name));
    fs::create_dir_all(&cache)?;
    for object in &policy.object {
        let target = destination.join(&object.path);
        let pointer = fs::read_to_string(&target)
            .with_context(|| format!("missing upstream LFS pointer {}", target.display()))?;
        if pointer != lfs_pointer(object) {
            bail!("upstream LFS pointer metadata mismatch for {}", object.path);
        }
        let payload = cache.join(&object.sha256);
        let valid_cached = payload.is_file()
            && payload.metadata()?.len() == object.size
            && performance::sha256_file(&payload)? == object.sha256;
        if !valid_cached {
            let temporary = cache.join(format!("{}.partial", object.sha256));
            remove_path_if_exists(&temporary)?;
            let url = policy.source.replace("{path}", &object.path);
            run_cmd(
                repo_root,
                "curl",
                &[
                    "--fail",
                    "--location",
                    "--silent",
                    "--show-error",
                    "--output",
                    temporary
                        .to_str()
                        .ok_or_else(|| anyhow!("invalid LFS cache path"))?,
                    &url,
                ],
            )?;
            if temporary.metadata()?.len() != object.size
                || performance::sha256_file(&temporary)? != object.sha256
            {
                bail!(
                    "downloaded LFS payload failed verification for {}",
                    object.path
                );
            }
            fs::rename(&temporary, &payload)?;
        }
        fs::copy(&payload, &target)?;
        set_mode(target, 0o644)?;
    }
    fs::remove_dir_all(&cache)?;
    Ok(())
}
