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
            // `branch` and `repo` describe where the pinned immutable
            // revision comes from.  An update verifies the tree against the
            // digest recorded at its last import and replaces it, so it needs
            // no history shared with the previous pin: a component may move
            // to the next release tag or to another repository (such as the
            // Linux stable tree).  The new repository is recorded in state.
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
    let committed_at = upstream_commit_time(&tmp)?;
    let policies = ComponentPolicies::load(repo_root, comp)?;
    let projection = policies.projection();
    let (upstream_tree, imported_tree_digest) = imported_tree_identity(&tmp, projection)?;

    clear_directory_contents_except(destination, &nested_component_paths(repo_root, comp)?)?;
    materialize_git_tree_exact(&tmp, "HEAD", destination, projection)?;
    apply_source_selection(destination, policies.source_selection.as_ref())?;
    hydrate_lfs_objects(repo_root, comp, destination, policies.lfs.as_ref())?;

    let state = SyncState {
        schema_version: 2,
        component: comp.name.clone(),
        repo: comp.repo.clone(),
        branch: comp.branch.clone(),
        imported_commit: commit.trim().to_owned(),
        ..policies.state(comp, upstream_tree, imported_tree_digest, committed_at)
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

/// Updates an imported component to its pinned revision.
///
/// Vendored trees are never edited in place (MattOS changes are patches
/// applied to build mirrors), so an update needs no upstream history: the
/// tree is verified against the digest recorded at its last import and then
/// replaced by the new commit's projected tree.  A tree that no longer
/// matches its import is refused with the modified paths listed, never
/// merged, so no conflict markers can reach an authoritative source tree.
fn update_component(
    repo_root: &Path,
    comp: &ComponentDef,
    destination: &Path,
    prior_state: &SyncState,
) -> Result<()> {
    let nested = nested_component_paths(repo_root, comp)?;
    let policies = ComponentPolicies::load(repo_root, comp)?;
    let projection = policies.projection();
    // Resolve the new pin first: its identity also validates the policies
    // against the new upstream tree, so a stale policy reports itself.
    let tmp_upstream = prepare_tmp_clone(repo_root, comp)?;
    let new_commit = run_cmd_capture(&tmp_upstream, "git", &["rev-parse", "HEAD"])?
        .trim()
        .to_owned();
    let committed_at = upstream_commit_time(&tmp_upstream)?;
    let (upstream_tree, imported_tree_digest) = imported_tree_identity(&tmp_upstream, projection)?;
    ensure_unmodified_since_import(
        repo_root,
        comp,
        destination,
        prior_state,
        projection,
        policies.lfs.as_ref(),
        &nested,
    )?;
    if prior_state.repo != comp.repo {
        println!(
            "{}: upstream repository changes from {} to {}",
            comp.name, prior_state.repo, comp.repo
        );
    }

    clear_directory_contents_except(destination, &nested)?;
    materialize_git_tree_exact(&tmp_upstream, "HEAD", destination, projection)?;
    apply_source_selection(destination, policies.source_selection.as_ref())?;
    hydrate_lfs_objects(repo_root, comp, destination, policies.lfs.as_ref())?;
    fs::remove_dir_all(&tmp_upstream)
        .with_context(|| format!("failed to remove {}", tmp_upstream.display()))?;

    let unchanged = new_commit == prior_state.imported_commit.trim();
    let mut state = SyncState {
        schema_version: 2,
        component: comp.name.clone(),
        repo: comp.repo.clone(),
        branch: comp.branch.clone(),
        imported_commit: new_commit,
        ..policies.state(comp, upstream_tree, imported_tree_digest, committed_at)
    };
    // Re-synchronizing an unchanged import is not a new import: keep its
    // timestamp so the state file does not churn.
    if unchanged && state.imported_tree_digest == prior_state.imported_tree_digest {
        state.imported_at_utc.clone_from(&prior_state.imported_at_utc);
    }
    write_sync_state(repo_root, &comp.name, &state)?;
    if unchanged {
        println!("Synchronized {} at unchanged commit {}", comp.name, state.imported_commit);
    } else {
        println!("Updated {} to commit {}", comp.name, state.imported_commit);
    }
    Ok(())
}

/// A component's source-selection, intentional-omission, gitlink, patch and
/// LFS policies, as recorded in its sync state.
struct ComponentPolicies {
    source_selection: Option<SourceSelectionPolicy>,
    source_selection_policy: String,
    source_selection_policy_sha256: String,
    omission: Option<OmissionPolicy>,
    intentional_omission_policy: String,
    gitlink_policy: String,
    patch_manifest: String,
    patch_manifest_sha256: String,
    lfs: Option<LfsHydrationPolicy>,
    lfs_policy: String,
    lfs_policy_sha256: String,
}

impl ComponentPolicies {
    fn load(repo_root: &Path, comp: &ComponentDef) -> Result<Self> {
        let (source_selection, source_selection_policy, source_selection_policy_sha256) =
            load_source_selection_policy(repo_root, comp)?;
        let (
            intentional_omission_policy,
            gitlink_policy,
            patch_manifest,
            patch_manifest_sha256,
            lfs_policy,
            lfs_policy_sha256,
        ) = component_provenance_policy(repo_root, &comp.name)?;
        let omission = load_intentional_omission_policy(repo_root, comp, &intentional_omission_policy)?;
        let lfs = load_lfs_hydration_policy(repo_root, comp, &lfs_policy, &lfs_policy_sha256)?;
        Ok(Self {
            source_selection,
            source_selection_policy,
            source_selection_policy_sha256,
            omission,
            intentional_omission_policy,
            gitlink_policy,
            patch_manifest,
            patch_manifest_sha256,
            lfs,
            lfs_policy,
            lfs_policy_sha256,
        })
    }

    fn projection(&self) -> TreeProjection<'_> {
        TreeProjection {
            selection: self.source_selection.as_ref(),
            omission: self.omission.as_ref(),
        }
    }

    /// Every state field except the component identity and commit.
    fn state(
        &self,
        comp: &ComponentDef,
        upstream_tree: String,
        imported_tree_digest: String,
        upstream_committed_at_utc: String,
    ) -> SyncState {
        SyncState {
            schema_version: 2,
            component: comp.name.clone(),
            repo: comp.repo.clone(),
            branch: comp.branch.clone(),
            imported_commit: String::new(),
            imported_at_utc: Utc::now().to_rfc3339(),
            sync_method: comp.sync.clone(),
            destination_path: comp.path.clone(),
            upstream_tree,
            imported_tree_digest_algorithm: if self.projection().is_selected() {
                SELECTED_IMPORTED_TREE_DIGEST_ALGORITHM
            } else {
                IMPORTED_TREE_DIGEST_ALGORITHM
            }
            .to_string(),
            imported_tree_digest,
            source_selection_policy: self.source_selection_policy.clone(),
            source_selection_policy_sha256: self.source_selection_policy_sha256.clone(),
            intentional_omission_policy: self.intentional_omission_policy.clone(),
            gitlink_policy: self.gitlink_policy.clone(),
            patch_manifest: self.patch_manifest.clone(),
            patch_manifest_sha256: self.patch_manifest_sha256.clone(),
            lfs_policy: self.lfs_policy.clone(),
            lfs_policy_sha256: self.lfs_policy_sha256.clone(),
            upstream_committed_at_utc: Some(upstream_committed_at_utc),
        }
    }
}

/// Committer time of `HEAD` in an upstream clone, as UTC RFC 3339.  A shallow
/// clone still has the commit object, so no history is needed.
fn upstream_commit_time(clone: &Path) -> Result<String> {
    let seconds = run_cmd_capture(clone, "git", &["show", "-s", "--format=%ct", "HEAD"])?;
    let seconds: i64 = seconds
        .trim()
        .parse()
        .with_context(|| format!("git reported an invalid commit time {seconds:?}"))?;
    let time = chrono::DateTime::from_timestamp(seconds, 0)
        .ok_or_else(|| anyhow!("commit time {seconds} is out of range"))?;
    Ok(time.format("%Y-%m-%dT%H:%M:%SZ").to_string())
}

/// How an upstream tree maps onto a component's imported tree.
#[derive(Clone, Copy)]
struct TreeProjection<'a> {
    selection: Option<&'a SourceSelectionPolicy>,
    omission: Option<&'a OmissionPolicy>,
}

impl TreeProjection<'_> {
    fn is_selected(&self) -> bool {
        self.selection.is_some() || self.omission.is_some()
    }

    /// The imported path of `upstream_path`, or `None` when it is not imported.
    fn map(&self, upstream_path: &str) -> Option<String> {
        let path = match self.omission {
            Some(policy) => policy.map(upstream_path)?,
            None => upstream_path.to_string(),
        };
        self.selection.is_none_or(|policy| policy.retains(&path)).then_some(path)
    }

    /// Whether a path in the imported tree lies where this projection imports
    /// nothing (a stale leftover of an earlier, wider projection).
    fn excludes_imported_path(&self, path: &str) -> bool {
        let upstream = match self.omission.and_then(|policy| policy.upstream_subtree.as_deref()) {
            Some(subtree) => format!("{}/{path}", subtree.trim_end_matches('/')),
            None => path.to_string(),
        };
        self.map(&upstream).is_none()
    }
}

/// Destinations of other components nested inside `comp` (for example a
/// gitlink replacement imported inside its parent).  They belong to their
/// own components and are never cleared, verified or replaced here.
fn nested_component_paths(repo_root: &Path, comp: &ComponentDef) -> Result<Vec<PathBuf>> {
    let Ok(sources) = read_sources(repo_root) else {
        return Ok(Vec::new());
    };
    let prefix = format!("{}/", comp.path.trim_end_matches('/'));
    Ok(sources
        .component
        .iter()
        .filter(|other| other.name != comp.name && other.path.starts_with(&prefix))
        .map(|other| PathBuf::from(&other.path[prefix.len()..]))
        .collect())
}

/// Git's blob identity of a file's bytes (or a symlink's target).
fn git_blob_id(payload: &[u8]) -> String {
    use sha1::{Digest as _, Sha1};
    let mut hasher = Sha1::new();
    hasher.update(format!("blob {}\0", payload.len()).as_bytes());
    hasher.update(payload);
    format!("{:x}", hasher.finalize())
}

/// `git ls-tree` records of the files under `directory` (relative paths,
/// sorted as Git sorts them), skipping `.git` and the `skipped` subtrees.
fn working_tree_records(directory: &Path, skipped: &[PathBuf]) -> Result<BTreeMap<String, (String, String)>> {
    let mut records = BTreeMap::new();
    let mut pending = vec![PathBuf::new()];
    while let Some(relative) = pending.pop() {
        for entry in fs::read_dir(directory.join(&relative))? {
            let entry = entry?;
            let child = relative.join(entry.file_name());
            if entry.file_name() == ".git" || skipped.contains(&child) {
                continue;
            }
            let path = directory.join(&child);
            let metadata = fs::symlink_metadata(&path)?;
            let name = child.to_string_lossy().into_owned();
            if metadata.file_type().is_symlink() {
                use std::os::unix::ffi::OsStrExt;
                let target = fs::read_link(&path)?;
                records.insert(name, ("120000".to_string(), git_blob_id(target.as_os_str().as_bytes())));
            } else if metadata.is_dir() {
                pending.push(child);
            } else {
                use std::os::unix::fs::PermissionsExt;
                let mode = if metadata.permissions().mode() & 0o100 != 0 { "100755" } else { "100644" };
                records.insert(name, (mode.to_string(), git_blob_id(&fs::read(&path)?)));
            }
        }
    }
    Ok(records)
}

fn records_digest<'a>(records: impl IntoIterator<Item = (&'a String, &'a (String, String))>) -> String {
    let mut digest = Sha256Hasher::new();
    for (path, (mode, blob)) in records {
        digest.update(format!("{mode} blob {blob}\t{path}").as_bytes());
        digest.update([0]);
    }
    format!("{:x}", digest.finalize())
}

/// Refuses an update when the vendored tree no longer matches the digest
/// recorded at its last import.  Besides the imported files, the tree may
/// hold only untracked, Git-ignored build residue and leftovers at paths
/// the projection now excludes; both are removed by the update.
fn ensure_unmodified_since_import(
    repo_root: &Path,
    comp: &ComponentDef,
    destination: &Path,
    prior_state: &SyncState,
    projection: TreeProjection<'_>,
    lfs: Option<&LfsHydrationPolicy>,
    nested: &[PathBuf],
) -> Result<()> {
    let mut records = working_tree_records(destination, nested)?;
    // Hydrated LFS files stand in for the pointer blobs the import recorded:
    // one whose content matches its prior pointer counts as that pointer.
    let lfs_paths = lfs
        .map(|policy| {
            policy
                .object
                .iter()
                .map(|object| object.path.clone())
                .filter(|path| records.contains_key(path))
                .collect::<BTreeSet<_>>()
        })
        .unwrap_or_default();
    if !lfs_paths.is_empty() {
        let (prior, pointers) = prior_upstream_tree(repo_root, comp, prior_state, projection, &lfs_paths)?;
        for path in &lfs_paths {
            let (Some(record), Some(pointer)) = (prior.get(path), pointers.get(path)) else {
                continue;
            };
            let oid = pointer
                .lines()
                .find_map(|line| line.strip_prefix("oid sha256:"))
                .unwrap_or_default();
            let content = format!("{:x}", Sha256Hasher::digest(fs::read(destination.join(path))?));
            if oid == content {
                records.insert(path.clone(), record.clone());
            }
        }
    }
    let ignored = outer_repository_paths(repo_root, &comp.path, &["--others", "--ignored", "--exclude-standard"])?;
    // Ignored, untracked files are residue unless they are upstream files the
    // outer repository happens to ignore; only the prior commit knows which.
    let ignored_present = records.keys().filter(|path| ignored.contains(*path)).cloned().collect::<Vec<_>>();
    let mut prior_tree = None;
    if !ignored_present.is_empty() {
        let tree = prior_upstream_records(repo_root, comp, prior_state, projection)?;
        records.retain(|path, _| !ignored.contains(path) || tree.contains_key(path));
        prior_tree = Some(tree);
    }
    if records_digest(&records) == prior_state.imported_tree_digest {
        return Ok(());
    }
    // Leftovers at paths the projection now excludes (from an earlier, wider
    // projection) are not edits; they are removed by the update.
    let without_stale = records
        .iter()
        .filter(|(path, _)| !projection.excludes_imported_path(path))
        .map(|(path, record)| (path.clone(), record.clone()))
        .collect::<BTreeMap<_, _>>();
    if records_digest(&without_stale) == prior_state.imported_tree_digest {
        return Ok(());
    }
    let prior = match prior_tree {
        Some(tree) => Some(tree),
        None => prior_upstream_records(repo_root, comp, prior_state, projection).ok(),
    };
    let mut details = Vec::new();
    if let Some(prior) = prior {
        for (path, record) in &records {
            match prior.get(path) {
                Some(expected) if expected == record => {}
                Some(_) => details.push(format!("modified {path}")),
                None => details.push(format!("added {path}")),
            }
        }
        details.extend(prior.keys().filter(|path| !records.contains_key(*path)).map(|path| format!("missing {path}")));
    }
    let shown = details.iter().take(20).cloned().collect::<Vec<_>>().join("\n  ");
    bail!(
        "{} no longer matches its import of {} (vendored trees must not be edited; MattOS changes belong in upstream/patches). \
         Restore the tree (for example `git checkout -- {}` and remove added files) and retry.{}{}",
        comp.name,
        prior_state.imported_commit,
        comp.path,
        if shown.is_empty() { String::new() } else { format!("\n  {shown}") },
        if details.len() > 20 { format!("\n  ... and {} more", details.len() - 20) } else { String::new() }
    )
}

/// Paths under `component_path` that `git ls-files <args>` lists in the
/// outer repository, relative to the component.
fn outer_repository_paths(repo_root: &Path, component_path: &str, args: &[&str]) -> Result<BTreeSet<String>> {
    let mut arguments = vec!["ls-files", "-z"];
    arguments.extend_from_slice(args);
    arguments.extend(["--", component_path]);
    let output = run_cmd_output(repo_root, "git", &arguments)?;
    if !output.status.success() {
        return Ok(BTreeSet::new());
    }
    let prefix = format!("{}/", component_path.trim_end_matches('/'));
    Ok(output
        .stdout
        .split(|byte| *byte == 0)
        .filter_map(|path| std::str::from_utf8(path).ok()?.strip_prefix(&prefix).map(str::to_string))
        .collect())
}

/// The projected records of the previously imported commit, fetched alone
/// (depth 1) from the repository it was imported from.
fn prior_upstream_records(
    repo_root: &Path,
    comp: &ComponentDef,
    prior_state: &SyncState,
    projection: TreeProjection<'_>,
) -> Result<BTreeMap<String, (String, String)>> {
    Ok(prior_upstream_tree(repo_root, comp, prior_state, projection, &BTreeSet::new())?.0)
}

/// The projected records of the previously imported commit, plus the text
/// of the blobs at `blob_paths` (imported paths).
fn prior_upstream_tree(
    repo_root: &Path,
    comp: &ComponentDef,
    prior_state: &SyncState,
    projection: TreeProjection<'_>,
    blob_paths: &BTreeSet<String>,
) -> Result<(BTreeMap<String, (String, String)>, BTreeMap<String, String>)> {
    let scratch = repo_root.join("upstream/.tmp").join(format!("{}-prior", comp.name));
    remove_path_if_exists(&scratch)?;
    fs::create_dir_all(&scratch)?;
    run_cmd(&scratch, "git", &["init", "-q", "--bare"])?;
    let commit = prior_state.imported_commit.trim();
    run_cmd(&scratch, "git", &["fetch", "-q", "--depth", "1", &prior_state.repo, commit])?;
    let listing = run_cmd_output(&scratch, "git", &["ls-tree", "-rz", commit])?;
    if !listing.status.success() {
        remove_path_if_exists(&scratch)?;
        bail!("failed to list prior import {commit} of {}", comp.name);
    }
    let mut records = BTreeMap::new();
    let mut blobs = BTreeMap::new();
    for record in listing.stdout.split(|byte| *byte == 0).filter(|record| !record.is_empty()) {
        let text = String::from_utf8_lossy(record);
        let Some((header, path)) = text.split_once('\t') else { continue };
        let fields = header.split_whitespace().collect::<Vec<_>>();
        if fields.len() != 3 || fields[0] == "160000" {
            continue;
        }
        if let Some(imported) = projection.map(path) {
            if blob_paths.contains(&imported) {
                blobs.insert(imported.clone(), run_cmd_capture(&scratch, "git", &["cat-file", "blob", fields[2]])?);
            }
            records.insert(imported, (fields[0].to_string(), fields[2].to_string()));
        }
    }
    remove_path_if_exists(&scratch)?;
    Ok((records, blobs))
}

/// Returns the immutable upstream Git tree object and a SHA-256 over the
/// canonical recursive `git ls-tree` records that have physical vendored-tree
/// representations. Gitlinks are excluded from the imported-tree digest and
/// are instead required to have an explicit replacement/exclusion policy.
fn imported_tree_identity(
    source_git: &Path,
    projection: TreeProjection<'_>,
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
    let mut imported = BTreeSet::new();
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
        let Some(mapped) = projection.map(&path) else {
            continue;
        };
        digest.update(&record[..=tab]);
        digest.update(mapped.as_bytes());
        digest.update([0]);
        imported.insert(path.into_owned());
    }
    if let Some(policy) = projection.omission {
        validate_omission_selection(policy, &imported)?;
    }
    Ok((upstream_tree, format!("{:x}", digest.finalize())))
}

/// Checks that an intentional-omission policy still describes the upstream
/// tree: every retained path exists and a subtree still has its expected
/// files, so an upstream rename cannot silently shrink the import.
fn validate_omission_selection(policy: &OmissionPolicy, imported: &BTreeSet<String>) -> Result<()> {
    if imported.is_empty() {
        bail!("{} intentional-omission policy selects no upstream files", policy.component);
    }
    for selector in policy.retained_paths.iter().flatten() {
        let selector = selector.trim_end_matches('/');
        if !imported.iter().any(|path| path == selector || path.starts_with(&format!("{selector}/"))) {
            bail!("{} retained path does not exist upstream: {selector}", policy.component);
        }
    }
    if let (Some(expected), Some(subtree)) = (&policy.expected_runtime_files, &policy.upstream_subtree) {
        let prefix = format!("{}/", subtree.trim_end_matches('/'));
        let actual = imported
            .iter()
            .filter_map(|path| path.strip_prefix(&prefix))
            .map(str::to_string)
            .collect::<BTreeSet<_>>();
        if actual != expected.iter().cloned().collect() {
            bail!("{} expected_runtime_files no longer matches the upstream subtree", policy.component);
        }
    }
    Ok(())
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

/// Removes everything in `dir` except `.git` and the `preserved` subtrees
/// (relative paths, possibly nested several levels deep).
fn clear_directory_contents_except(dir: &Path, preserved: &[PathBuf]) -> Result<()> {
    if !dir.exists() {
        return Ok(());
    }
    for entry in
        fs::read_dir(dir).with_context(|| format!("failed to read directory: {}", dir.display()))?
    {
        let entry = entry?;
        let p = entry.path();
        let name = PathBuf::from(entry.file_name());
        if name == Path::new(".git") || preserved.contains(&name) {
            continue;
        }
        let deeper = preserved
            .iter()
            .filter_map(|path| path.strip_prefix(&name).ok())
            .filter(|rest| !rest.as_os_str().is_empty())
            .map(Path::to_path_buf)
            .collect::<Vec<_>>();
        let metadata = fs::symlink_metadata(&p)?;
        if metadata.is_dir() && !deeper.is_empty() {
            clear_directory_contents_except(&p, &deeper)?;
        } else if metadata.is_dir() {
            fs::remove_dir_all(&p)
                .with_context(|| format!("failed to remove directory: {}", p.display()))?;
        } else {
            fs::remove_file(&p)
                .with_context(|| format!("failed to remove file: {}", p.display()))?;
        }
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

/// Materializes Git blob bytes and modes directly, bypassing checkout-time
/// attributes such as `eol=crlf`, host clean/smudge filters, and autocrlf.
/// Authoritative imported trees must represent the pinned Git tree itself,
/// not a host-specific working-tree projection of it.
fn materialize_git_tree_exact(
    source_git: &Path,
    treeish: &str,
    destination: &Path,
    projection: TreeProjection<'_>,
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
        let Some(path) = projection.map(path) else {
            continue;
        };
        let path = path.as_str();
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
