fn import_sources(
    repo_root: &Path,
    all: bool,
    component: Option<String>,
    update: bool,
) -> Result<()> {
    let sources = read_sources(repo_root)?;
    let selected = select_components(&sources.component, all, component)?;

    for comp in selected {
        import_component(repo_root, comp, update)?;
    }

    Ok(())
}

fn read_sources(repo_root: &Path) -> Result<Sources> {
    let path = repo_root.join("upstream/sources.toml");
    let text = fs::read_to_string(&path)
        .with_context(|| format!("failed to read sources file: {}", path.display()))?;
    toml::from_str(&text).context("failed to parse upstream/sources.toml")
}

fn select_components<'a>(
    components: &'a [ComponentDef],
    all: bool,
    component: Option<String>,
) -> Result<Vec<&'a ComponentDef>> {
    if all {
        return Ok(components.iter().collect());
    }

    if let Some(name) = component {
        if let Some(found) = components.iter().find(|c| c.name == name) {
            return Ok(vec![found]);
        }
        bail!("unknown component: {name}");
    }

    bail!("pass --all or --component <name>")
}

fn import_component(repo_root: &Path, comp: &ComponentDef, update: bool) -> Result<()> {
    println!(
        "Importing {} from {} ({})",
        comp.name, comp.repo, comp.branch
    );
    validate_component_name(&comp.name)?;
    let destination = resolve_component_destination(repo_root, &comp.path)?;

    fs::create_dir_all(&destination)
        .with_context(|| format!("failed to create destination: {}", destination.display()))?;

    if update {
        if let Some(prior_state) = read_sync_state(repo_root, &comp.name)? {
            if prior_state.repo != comp.repo || prior_state.branch != comp.branch {
                bail!(
                    "state mismatch for {} (repo/branch changed); inspect upstream/state/{}.toml",
                    comp.name,
                    comp.name
                )
            }
            update_component(repo_root, comp, &destination, &prior_state)
        } else if is_scaffold_directory(&destination)? {
            println!(
                "No existing sync state for {}; performing initial import into scaffold directory",
                comp.name
            );
            initial_import_component(repo_root, comp, &destination)
        } else {
            bail!(
                "missing upstream state for {}; run initial import before --update",
                comp.name
            )
        }
    } else {
        initial_import_component(repo_root, comp, &destination)
    }
}

fn is_scaffold_directory(dir: &Path) -> Result<bool> {
    if !dir.exists() {
        return Ok(true);
    }

    for entry in fs::read_dir(dir).with_context(|| format!("failed to read {}", dir.display()))? {
        let entry = entry?;
        if is_safe_placeholder_entry(&entry)? {
            continue;
        }
        return Ok(false);
    }
    Ok(true)
}

fn is_safe_placeholder_entry(entry: &fs::DirEntry) -> Result<bool> {
    let name = entry.file_name();
    if !SAFE_IMPORT_PLACEHOLDER_FILES
        .iter()
        .any(|allowed| name == OsStr::new(allowed))
    {
        return Ok(false);
    }
    let meta = entry.file_type().with_context(|| {
        format!(
            "failed to inspect placeholder type for {}",
            entry.path().display()
        )
    })?;
    Ok(meta.is_file())
}

fn initial_import_component(
    repo_root: &Path,
    comp: &ComponentDef,
    destination: &Path,
) -> Result<()> {
    assert_initial_destination_safe(destination)?;

    let tmp = prepare_tmp_clone(repo_root, comp)?;
    let commit = run_cmd_capture(&tmp, "git", &["rev-parse", "HEAD"])?;
    let (source_selection, source_selection_policy, source_selection_policy_sha256) =
        load_source_selection_policy(repo_root, comp)?;
    let (upstream_tree, imported_tree_digest) =
        imported_tree_identity(&tmp, source_selection.as_ref())?;
    let (
        intentional_omission_policy,
        gitlink_policy,
        patch_manifest,
        patch_manifest_sha256,
        lfs_policy_name,
        lfs_policy_sha256,
    ) = component_provenance_policy(repo_root, &comp.name)?;
    let lfs_policy =
        load_lfs_hydration_policy(repo_root, comp, &lfs_policy_name, &lfs_policy_sha256)?;

    clear_directory_contents(destination)?;
    materialize_git_tree_exact(&tmp, "HEAD", destination, source_selection.as_ref())?;
    apply_source_selection(destination, source_selection.as_ref())?;
    hydrate_lfs_objects(repo_root, comp, destination, lfs_policy.as_ref())?;

    let state = SyncState {
        schema_version: 2,
        component: comp.name.clone(),
        repo: comp.repo.clone(),
        branch: comp.branch.clone(),
        imported_commit: commit.trim().to_owned(),
        imported_at_utc: Utc::now().to_rfc3339(),
        sync_method: comp.sync.clone(),
        destination_path: comp.path.clone(),
        upstream_tree,
        imported_tree_digest_algorithm: if source_selection.is_some() {
            SELECTED_IMPORTED_TREE_DIGEST_ALGORITHM
        } else {
            IMPORTED_TREE_DIGEST_ALGORITHM
        }
        .to_string(),
        imported_tree_digest,
        source_selection_policy,
        source_selection_policy_sha256,
        intentional_omission_policy,
        gitlink_policy,
        patch_manifest,
        patch_manifest_sha256,
        lfs_policy: lfs_policy_name,
        lfs_policy_sha256,
    };
    write_sync_state(repo_root, &comp.name, &state)?;

    fs::remove_dir_all(&tmp)
        .with_context(|| format!("failed to remove temporary directory: {}", tmp.display()))?;

    println!("Imported {} at commit {}", comp.name, state.imported_commit);
    Ok(())
}

fn assert_initial_destination_safe(destination: &Path) -> Result<()> {
    if !destination.exists() {
        return Ok(());
    }

    let mut unsafe_entries = Vec::new();
    for entry in fs::read_dir(destination)
        .with_context(|| format!("failed to inspect destination: {}", destination.display()))?
    {
        let entry = entry?;
        if is_safe_placeholder_entry(&entry)? {
            continue;
        }
        unsafe_entries.push(entry.file_name().to_string_lossy().to_string());
    }

    if !unsafe_entries.is_empty() {
        unsafe_entries.sort();
        bail!(
            "initial import refused: destination {} contains non-placeholder files: {}",
            destination.display(),
            unsafe_entries.join(", ")
        )
    }

    Ok(())
}

fn update_component(
    repo_root: &Path,
    comp: &ComponentDef,
    destination: &Path,
    prior_state: &SyncState,
) -> Result<()> {
    let tmp_upstream = prepare_tmp_clone(repo_root, comp)?;
    // The three-way sync below needs the prior imported commit as well as the
    // new branch head. Hydrate a genuinely shallow clone before constructing
    // the merge. Local fixture repositories ignore --depth during clone, so
    // asking Git to unshallow them is an error rather than a harmless no-op.
    let shallow = run_cmd_capture(
        &tmp_upstream,
        "git",
        &["rev-parse", "--is-shallow-repository"],
    )?;
    if shallow.trim() == "true" {
        run_cmd(&tmp_upstream, "git", &["fetch", "--unshallow", "origin"])?;
    }
    let new_commit = run_cmd_capture(&tmp_upstream, "git", &["rev-parse", "HEAD"])?;
    let (source_selection, source_selection_policy, source_selection_policy_sha256) =
        load_source_selection_policy(repo_root, comp)?;
    let (upstream_tree, imported_tree_digest) =
        imported_tree_identity(&tmp_upstream, source_selection.as_ref())?;
    let (
        intentional_omission_policy,
        gitlink_policy,
        patch_manifest,
        patch_manifest_sha256,
        lfs_policy_name,
        lfs_policy_sha256,
    ) = component_provenance_policy(repo_root, &comp.name)?;
    let lfs_policy =
        load_lfs_hydration_policy(repo_root, comp, &lfs_policy_name, &lfs_policy_sha256)?;

    let old_commit = prior_state.imported_commit.trim();
    if new_commit.trim() == old_commit {
        clear_directory_contents(destination)?;
        materialize_git_tree_exact(
            &tmp_upstream,
            "HEAD",
            destination,
            source_selection.as_ref(),
        )?;
        apply_source_selection(destination, source_selection.as_ref())?;
        hydrate_lfs_objects(repo_root, comp, destination, lfs_policy.as_ref())?;
        let state = SyncState {
            schema_version: 2,
            component: comp.name.clone(),
            repo: comp.repo.clone(),
            branch: comp.branch.clone(),
            imported_commit: new_commit.trim().to_owned(),
            imported_at_utc: Utc::now().to_rfc3339(),
            sync_method: comp.sync.clone(),
            destination_path: comp.path.clone(),
            upstream_tree,
            imported_tree_digest_algorithm: if source_selection.is_some() {
                SELECTED_IMPORTED_TREE_DIGEST_ALGORITHM
            } else {
                IMPORTED_TREE_DIGEST_ALGORITHM
            }
            .to_string(),
            imported_tree_digest,
            source_selection_policy,
            source_selection_policy_sha256,
            intentional_omission_policy,
            gitlink_policy,
            patch_manifest,
            patch_manifest_sha256,
            lfs_policy: lfs_policy_name,
            lfs_policy_sha256,
        };
        write_sync_state(repo_root, &comp.name, &state)?;
        fs::remove_dir_all(&tmp_upstream)
            .with_context(|| format!("failed to remove {}", tmp_upstream.display()))?;
        println!(
            "Synchronized {} at unchanged commit {}",
            comp.name, state.imported_commit
        );
        return Ok(());
    }
    run_cmd(
        &tmp_upstream,
        "git",
        &["fetch", "--depth", "1", "origin", old_commit],
    )?;

    let tmp_root = repo_root.join("upstream/.tmp");
    let tmp_merge = tmp_root.join(format!("{}-merge", comp.name));
    if tmp_merge.exists() {
        fs::remove_dir_all(&tmp_merge)
            .with_context(|| format!("failed to clean {}", tmp_merge.display()))?;
    }
    fs::create_dir_all(&tmp_merge)
        .with_context(|| format!("failed to create {}", tmp_merge.display()))?;

    run_cmd(&tmp_merge, "git", &["init"])?;
    run_cmd(
        &tmp_merge,
        "git",
        &[
            "remote",
            "add",
            "upstream",
            tmp_upstream
                .to_str()
                .ok_or_else(|| anyhow!("invalid path: {}", tmp_upstream.display()))?,
        ],
    )?;
    run_cmd(&tmp_merge, "git", &["fetch", "upstream", old_commit])?;
    run_cmd(&tmp_merge, "git", &["fetch", "upstream", new_commit.trim()])?;
    run_cmd(
        &tmp_merge,
        "git",
        &["checkout", "-q", "-b", "local", old_commit],
    )?;

    clear_directory_contents(&tmp_merge)?;
    copy_tree_excluding_dotgit(destination, &tmp_merge)?;
    restore_lfs_pointers_for_merge(&tmp_merge, lfs_policy.as_ref())?;
    run_cmd(&tmp_merge, "git", &["add", "-A"])?;
    let local_status = run_cmd_capture(&tmp_merge, "git", &["status", "--porcelain"])?;
    if !local_status.is_empty() {
        run_cmd(
            &tmp_merge,
            "git",
            &[
                "-c",
                "user.name=MattOS Sync Bot",
                "-c",
                "user.email=syncbot@example.invalid",
                "commit",
                "-m",
                "MattOS local snapshot before upstream sync",
            ],
        )?;
    }

    let merge_status = run_cmd_status(
        &tmp_merge,
        "git",
        &["merge", "--no-commit", "--no-ff", new_commit.trim()],
    )?;
    let has_conflicts = merge_status.code() == Some(1);
    if !merge_status.success() && !has_conflicts {
        bail!(
            "sync merge failed unexpectedly with status {}",
            merge_status
                .code()
                .map(|c| c.to_string())
                .unwrap_or_else(|| "unknown".to_string())
        );
    }

    clear_directory_contents(destination)?;
    if has_conflicts {
        copy_tree_excluding_dotgit(&tmp_merge, destination)?;
    } else {
        let merged_tree = run_cmd_capture(&tmp_merge, "git", &["write-tree"])?;
        materialize_git_tree_exact(
            &tmp_merge,
            merged_tree.trim(),
            destination,
            source_selection.as_ref(),
        )?;
    }
    apply_source_selection(destination, source_selection.as_ref())?;
    if !has_conflicts {
        hydrate_lfs_objects(repo_root, comp, destination, lfs_policy.as_ref())?;
    }

    fs::remove_dir_all(&tmp_upstream)
        .with_context(|| format!("failed to remove {}", tmp_upstream.display()))?;
    fs::remove_dir_all(&tmp_merge)
        .with_context(|| format!("failed to remove {}", tmp_merge.display()))?;

    if has_conflicts {
        bail!(
            "upstream sync for {} produced merge conflicts under {}; resolve conflicts and rerun --update",
            comp.name,
            comp.path
        );
    }

    let state = SyncState {
        schema_version: 2,
        component: comp.name.clone(),
        repo: comp.repo.clone(),
        branch: comp.branch.clone(),
        imported_commit: new_commit.trim().to_owned(),
        imported_at_utc: Utc::now().to_rfc3339(),
        sync_method: comp.sync.clone(),
        destination_path: comp.path.clone(),
        upstream_tree,
        imported_tree_digest_algorithm: if source_selection.is_some() {
            SELECTED_IMPORTED_TREE_DIGEST_ALGORITHM
        } else {
            IMPORTED_TREE_DIGEST_ALGORITHM
        }
        .to_string(),
        imported_tree_digest,
        source_selection_policy,
        source_selection_policy_sha256,
        intentional_omission_policy,
        gitlink_policy,
        patch_manifest,
        patch_manifest_sha256,
        lfs_policy: lfs_policy_name,
        lfs_policy_sha256,
    };
    write_sync_state(repo_root, &comp.name, &state)?;

    println!("Updated {} to commit {}", comp.name, state.imported_commit);
    Ok(())
}

/// Returns the immutable upstream Git tree object and a SHA-256 over the
/// canonical recursive `git ls-tree` records that have physical vendored-tree
/// representations. Gitlinks are excluded from the imported-tree digest and
/// are instead required to have an explicit replacement/exclusion policy.
fn imported_tree_identity(
    source_git: &Path,
    source_selection: Option<&SourceSelectionPolicy>,
) -> Result<(String, String)> {
    let upstream_tree = run_cmd_capture(source_git, "git", &["rev-parse", "HEAD^{tree}"])?
        .trim()
        .to_string();
    let output = run_cmd_output(source_git, "git", &["ls-tree", "-rz", "HEAD"])?;
    if !output.status.success() {
        bail!(
            "failed to enumerate imported upstream tree in {}",
            source_git.display()
        );
    }
    let mut digest = Sha256Hasher::new();
    for record in output
        .stdout
        .split(|byte| *byte == 0)
        .filter(|record| !record.is_empty())
    {
        if record.starts_with(b"160000 ") {
            continue;
        }
        let Some(tab) = record.iter().position(|byte| *byte == b'\t') else {
            bail!("malformed git ls-tree record in {}", source_git.display());
        };
        let path = String::from_utf8_lossy(&record[tab + 1..]);
        if source_selection.is_some_and(|policy| !policy.retains(&path)) {
            continue;
        }
        digest.update(record);
        digest.update([0]);
    }
    Ok((upstream_tree, format!("{:x}", digest.finalize())))
}

include!("selection.rs");
include!("provenance.rs");
include!("lfs.rs");
fn validate_component_name(name: &str) -> Result<()> {
    if name.is_empty() {
        bail!("component name must not be empty")
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        bail!("component name contains unsupported characters: {name}")
    }
    Ok(())
}

fn resolve_component_destination(repo_root: &Path, rel_path: &str) -> Result<PathBuf> {
    if rel_path.contains('\\') {
        bail!("component path must use forward slashes only: {rel_path}")
    }

    let rel = Path::new(rel_path);
    if rel.is_absolute() {
        bail!("component path must be relative: {rel_path}")
    }
    for piece in rel.components() {
        match piece {
            Component::Normal(_) | Component::CurDir => {}
            Component::ParentDir => bail!("component path cannot contain '..': {rel_path}"),
            Component::RootDir | Component::Prefix(_) => {
                bail!("component path has invalid prefix/root: {rel_path}")
            }
        }
    }

    let joined = repo_root.join(rel);
    if !joined.starts_with(repo_root) {
        bail!("component path escapes repository root: {rel_path}")
    }
    Ok(joined)
}

fn read_sync_state(repo_root: &Path, name: &str) -> Result<Option<SyncState>> {
    let path = repo_root
        .join("upstream/state")
        .join(format!("{name}.toml"));
    if !path.exists() {
        return Ok(None);
    }
    let body = fs::read_to_string(&path)
        .with_context(|| format!("failed to read sync state: {}", path.display()))?;
    let state = toml::from_str::<SyncState>(&body)
        .with_context(|| format!("failed to parse sync state: {}", path.display()))?;
    Ok(Some(state))
}

fn prepare_tmp_clone(repo_root: &Path, comp: &ComponentDef) -> Result<PathBuf> {
    let tmp_base = repo_root.join("upstream/.tmp");
    fs::create_dir_all(&tmp_base).context("failed to create temporary import directory")?;
    let tmp = tmp_base.join(format!("{}-clone", comp.name));
    if tmp.exists() {
        fs::remove_dir_all(&tmp)
            .with_context(|| format!("failed to remove previous temp dir: {}", tmp.display()))?;
    }

    run_cmd(
        repo_root,
        "git",
        &[
            "clone",
            "-c",
            "core.autocrlf=false",
            "--no-checkout",
            "--depth",
            "1",
            "--branch",
            &comp.branch,
            &comp.repo,
            tmp.to_str().ok_or_else(|| anyhow!("invalid temp path"))?,
        ],
    )?;
    if let Some(revision) = comp.revision.as_deref() {
        run_cmd(&tmp, "git", &["fetch", "--depth", "1", "origin", revision])?;
        run_cmd(&tmp, "git", &["checkout", "--detach", revision])?;
    } else {
        let remote_branch = format!("origin/{}", comp.branch);
        run_cmd(&tmp, "git", &["checkout", "--detach", &remote_branch])?;
    }

    Ok(tmp)
}

fn clear_directory_contents(dir: &Path) -> Result<()> {
    if !dir.exists() {
        return Ok(());
    }
    for entry in
        fs::read_dir(dir).with_context(|| format!("failed to read directory: {}", dir.display()))?
    {
        let entry = entry?;
        let p = entry.path();
        if p.file_name() == Some(OsStr::new(".git")) {
            continue;
        }
        if p.is_dir() {
            fs::remove_dir_all(&p)
                .with_context(|| format!("failed to remove directory: {}", p.display()))?;
        } else {
            fs::remove_file(&p)
                .with_context(|| format!("failed to remove file: {}", p.display()))?;
        }
    }
    Ok(())
}

/// Materializes Git blob bytes and modes directly, bypassing checkout-time
/// attributes such as `eol=crlf`, host clean/smudge filters, and autocrlf.
/// Authoritative imported trees must represent the pinned Git tree itself,
/// not a host-specific working-tree projection of it.
fn materialize_git_tree_exact(
    source_git: &Path,
    treeish: &str,
    destination: &Path,
    source_selection: Option<&SourceSelectionPolicy>,
) -> Result<()> {
    fs::create_dir_all(destination)?;
    let tree = run_cmd_output(source_git, "git", &["ls-tree", "-rz", treeish])?;
    if !tree.status.success() {
        bail!(
            "failed to enumerate Git tree {treeish} in {}",
            source_git.display()
        );
    }

    let mut objects = Vec::new();
    for record in tree
        .stdout
        .split(|byte| *byte == 0)
        .filter(|r| !r.is_empty())
    {
        let tab = record
            .iter()
            .position(|byte| *byte == b'\t')
            .ok_or_else(|| anyhow!("malformed git ls-tree record"))?;
        let header = std::str::from_utf8(&record[..tab]).context("non-UTF-8 tree header")?;
        let mut fields = header.split_whitespace();
        let mode = fields.next().ok_or_else(|| anyhow!("missing tree mode"))?;
        let kind = fields
            .next()
            .ok_or_else(|| anyhow!("missing object kind"))?;
        let object = fields.next().ok_or_else(|| anyhow!("missing object id"))?;
        let path =
            std::str::from_utf8(&record[tab + 1..]).context("imported source path is not UTF-8")?;
        if mode == "160000" || kind == "commit" {
            continue;
        }
        if source_selection.is_some_and(|policy| !policy.retains(path)) {
            continue;
        }
        if Path::new(path).is_absolute()
            || Path::new(path)
                .components()
                .any(|part| matches!(part, Component::ParentDir))
        {
            bail!("Git tree path escapes import destination: {path}");
        }
        objects.push((mode.to_string(), object.to_string(), path.to_string()));
    }

    let mut child = Command::new("git")
        .args(["cat-file", "--batch"])
        .current_dir(source_git)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .context("failed to start git cat-file --batch")?;
    let mut input = child.stdin.take().expect("piped cat-file stdin");
    let mut output = BufReader::new(child.stdout.take().expect("piped cat-file stdout"));

    for (mode, object, relative) in objects {
        writeln!(input, "{object}")?;
        input.flush()?;
        let mut header = String::new();
        output.read_line(&mut header)?;
        let mut fields = header.split_whitespace();
        let actual_object = fields.next().unwrap_or_default();
        let kind = fields.next().unwrap_or_default();
        let size = fields
            .next()
            .ok_or_else(|| anyhow!("missing cat-file size for {relative}"))?
            .parse::<usize>()
            .with_context(|| format!("invalid cat-file size for {relative}"))?;
        if actual_object != object || kind != "blob" {
            bail!(
                "unexpected cat-file response for {relative}: {}",
                header.trim()
            );
        }
        let mut payload = vec![0; size];
        output.read_exact(&mut payload)?;
        let mut terminator = [0_u8; 1];
        output.read_exact(&mut terminator)?;
        if terminator[0] != b'\n' {
            bail!("malformed cat-file terminator for {relative}");
        }

        let target = destination.join(&relative);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        remove_path_if_exists(&target)?;
        if mode == "120000" {
            #[cfg(unix)]
            std::os::unix::fs::symlink(OsString::from_vec(payload), &target)?;
            #[cfg(not(unix))]
            bail!("exact symlink imports require Unix");
        } else {
            fs::write(&target, payload)?;
            set_mode(target, if mode == "100755" { 0o755 } else { 0o644 })?;
        }
    }
    drop(input);
    let status = child.wait()?;
    if !status.success() {
        bail!("git cat-file failed while materializing {treeish}");
    }
    Ok(())
}

fn copy_tree_excluding_dotgit(src: &Path, dst: &Path) -> Result<()> {
    fs::create_dir_all(dst)
        .with_context(|| format!("failed to create copy destination: {}", dst.display()))?;
    for entry in fs::read_dir(src)
        .with_context(|| format!("failed to read source dir: {}", src.display()))?
    {
        let entry = entry?;
        let from = entry.path();
        let name = entry.file_name();
        let metadata = fs::symlink_metadata(&from)
            .with_context(|| format!("failed to read metadata: {}", from.display()))?;

        if name == OsStr::new(".git") {
            continue;
        }

        let to = dst.join(&name);
        if metadata.file_type().is_symlink() {
            remove_path_if_exists(&to)?;
            copy_symlink(&from, &to)?;
        } else if metadata.is_dir() {
            if to.symlink_metadata().is_ok() && !to.is_dir() {
                remove_path_if_exists(&to)?;
            }
            copy_tree_excluding_dotgit(&from, &to)?;
        } else {
            if to.symlink_metadata().is_ok() && !to.is_file() {
                remove_path_if_exists(&to)?;
            }
            fs::copy(&from, &to).with_context(|| {
                format!("failed to copy {} to {}", from.display(), to.display())
            })?;
            preserve_permissions(&metadata, &to)?;
        }
    }
    Ok(())
}

/// Copies the authoritative working-tree inputs for an imported component into
/// an output-owned source mirror. Tracked modifications and non-ignored
/// untracked inputs are preserved; ignored build residue is deliberately not.
fn copy_imported_working_tree(
    repo_root: &Path,
    source_relative: &Path,
    destination: &Path,
) -> Result<()> {
    if source_relative.is_absolute()
        || source_relative
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        bail!(
            "imported source path must be repository-relative: {}",
            source_relative.display()
        );
    }
    let source = repo_root.join(source_relative);
    if !source.is_dir() {
        bail!("imported source directory missing: {}", source.display());
    }

    let output = Command::new("git")
        .args([
            "ls-files",
            "-z",
            "--cached",
            "--others",
            "--exclude-standard",
            "--",
        ])
        .arg(source_relative)
        .current_dir(repo_root)
        .output()
        .context("failed to enumerate authoritative imported-source inputs")?;
    if !output.status.success() {
        bail!(
            "git could not enumerate imported source {}: {}",
            source_relative.display(),
            output.status
        );
    }

    remove_path_if_exists(destination)?;
    fs::create_dir_all(destination)
        .with_context(|| format!("failed to create {}", destination.display()))?;
    for raw in output
        .stdout
        .split(|byte| *byte == 0)
        .filter(|raw| !raw.is_empty())
    {
        let repository_path = PathBuf::from(String::from_utf8_lossy(raw).into_owned());
        let relative = repository_path
            .strip_prefix(source_relative)
            .with_context(|| {
                format!(
                    "git returned {} outside imported source {}",
                    repository_path.display(),
                    source_relative.display()
                )
            })?;
        let from = repo_root.join(&repository_path);
        let Ok(metadata) = fs::symlink_metadata(&from) else {
            // A deleted tracked file is an authoritative working-tree deletion.
            continue;
        };
        let to = destination.join(relative);
        if let Some(parent) = to.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
        if metadata.file_type().is_symlink() {
            copy_symlink(&from, &to)?;
        } else if metadata.is_file() {
            fs::copy(&from, &to).with_context(|| {
                format!("failed to copy {} to {}", from.display(), to.display())
            })?;
            preserve_permissions(&metadata, &to)?;
        }
    }
    Ok(())
}

include!("patches.rs");
fn validated_repo_relative_path(value: &str) -> Result<&Path> {
    let path = Path::new(value);
    if path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        bail!("provenance path is not a safe repository-relative path: {value}");
    }
    Ok(path)
}

fn copy_tree_excluding_package_owned(
    src: &Path,
    rootfs: &Path,
    owned: &BTreeSet<PathBuf>,
) -> Result<()> {
    fn copy_inner(src: &Path, dst: &Path, rootfs: &Path, owned: &BTreeSet<PathBuf>) -> Result<()> {
        fs::create_dir_all(dst)
            .with_context(|| format!("failed to create copy destination: {}", dst.display()))?;
        let mut entries = fs::read_dir(src)?.collect::<std::io::Result<Vec<_>>>()?;
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            let from = entry.path();
            if entry.file_name() == OsStr::new(".git") {
                continue;
            }
            let to = dst.join(entry.file_name());
            let metadata = fs::symlink_metadata(&from)?;
            if metadata.is_dir() {
                copy_inner(&from, &to, rootfs, owned)?;
                continue;
            }
            let rel = to.strip_prefix(rootfs)?;
            if owned.contains(rel) {
                continue;
            }
            if metadata.file_type().is_symlink() {
                copy_symlink(&from, &to)?;
            } else {
                fs::copy(&from, &to)?;
                preserve_permissions(&metadata, &to)?;
            }
        }
        Ok(())
    }

    copy_inner(src, rootfs, rootfs, owned)
}

#[cfg(unix)]
fn copy_symlink(from: &Path, to: &Path) -> Result<()> {
    use std::os::unix::fs::symlink;

    let target = fs::read_link(from)
        .with_context(|| format!("failed to read symlink {}", from.display()))?;
    symlink(&target, to).with_context(|| format!("failed to create symlink {}", to.display()))?;
    Ok(())
}

#[cfg(not(unix))]
fn copy_symlink(from: &Path, to: &Path) -> Result<()> {
    let target = fs::read_link(from)
        .with_context(|| format!("failed to read symlink {}", from.display()))?;
    let parent = to
        .parent()
        .ok_or_else(|| anyhow!("missing parent for {}", to.display()))?;
    fs::create_dir_all(parent)
        .with_context(|| format!("failed to create parent {}", parent.display()))?;
    let resolved = from
        .parent()
        .ok_or_else(|| anyhow!("missing parent for {}", from.display()))?
        .join(target);
    fs::copy(&resolved, to)
        .with_context(|| format!("failed to copy symlink fallback {}", resolved.display()))?;
    Ok(())
}

#[cfg(unix)]
fn preserve_permissions(metadata: &fs::Metadata, to: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let mode = metadata.permissions().mode();
    fs::set_permissions(to, fs::Permissions::from_mode(mode))
        .with_context(|| format!("failed to set permissions on {}", to.display()))?;
    Ok(())
}

#[cfg(not(unix))]
fn preserve_permissions(_metadata: &fs::Metadata, _to: &Path) -> Result<()> {
    Ok(())
}

fn write_sync_state(repo_root: &Path, name: &str, state: &SyncState) -> Result<()> {
    let dir = repo_root.join("upstream/state");
    fs::create_dir_all(&dir).context("failed to create upstream/state")?;
    let path = dir.join(format!("{name}.toml"));
    let temp_path = dir.join(format!("{name}.toml.tmp"));
    let body = toml::to_string_pretty(state).context("failed to serialize sync state")?;
    fs::write(&temp_path, body).with_context(|| {
        format!(
            "failed to write temporary sync state: {}",
            temp_path.display()
        )
    })?;
    fs::rename(&temp_path, &path)
        .with_context(|| format!("failed to publish sync state: {}", path.display()))?;
    Ok(())
}
