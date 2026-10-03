// Disk usage report and safe pruning of reclaimable build-tree leftovers.

/// One reclaimable path and why it is safe to remove.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Reclaimable {
    path: PathBuf,
    bytes: u64,
    reason: &'static str,
}

/// Scratch under `out/tmp` older than this is reclaimable.
const DISK_TMP_MAX_AGE: std::time::Duration = std::time::Duration::from_secs(24 * 60 * 60);

fn disk_command(repo_root: &Path, command: DiskCommands) -> Result<()> {
    match command {
        DiskCommands::Report => disk_report(repo_root),
        DiskCommands::Prune { dry_run } => disk_prune(repo_root, dry_run),
    }
}

fn disk_usage(path: &Path) -> u64 {
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return 0;
    };
    if !metadata.is_dir() {
        return std::os::unix::fs::MetadataExt::blocks(&metadata) * 512;
    }
    let mut total = std::os::unix::fs::MetadataExt::blocks(&metadata) * 512;
    if let Ok(entries) = fs::read_dir(path) {
        for entry in entries.flatten() {
            total += disk_usage(&entry.path());
        }
    }
    total
}

fn human_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 { format!("{bytes} B") } else { format!("{value:.1} {}", UNITS[unit]) }
}

/// The build tool's own Rust source, read at run time: a build directory
/// whose name it never mentions belongs to no current recipe.
fn build_tool_source_text(repo_root: &Path) -> Result<String> {
    fn collect(directory: &Path, text: &mut String) -> Result<()> {
        for entry in fs::read_dir(directory)? {
            let path = entry?.path();
            if path.is_dir() {
                collect(&path, text)?;
            } else if path.extension().is_some_and(|extension| extension == "rs") {
                text.push_str(&fs::read_to_string(&path)?);
                text.push('\n');
            }
        }
        Ok(())
    }
    let mut text = String::new();
    collect(&repo_root.join("src/tools/mattos-build/src"), &mut text)?;
    Ok(text)
}

/// `name` from a `.<name>.building-<pid>` temporary sibling whose process no
/// longer exists (an interrupted publication).
fn abandoned_temporary(name: &str) -> bool {
    let Some((_, pid)) = name.rsplit_once(".building-") else {
        return false;
    };
    let Ok(pid) = pid.parse::<u32>() else {
        return false;
    };
    !Path::new("/proc").join(pid.to_string()).exists()
}

/// Everything `disk prune` would remove, with sizes.
fn reclaimable_paths(repo_root: &Path, stage_ids: &BTreeSet<String>, source: &str) -> Result<Vec<Reclaimable>> {
    let mut found = Vec::new();
    let mut add = |path: PathBuf, reason: &'static str| {
        let bytes = disk_usage(&path);
        found.push(Reclaimable { path, bytes, reason });
    };
    let build = repo_root.join("out/build");
    if let Ok(entries) = fs::read_dir(&build) {
        let mut entries = entries.flatten().map(|entry| entry.path()).collect::<Vec<_>>();
        entries.sort();
        for path in entries {
            let name = path.file_name().unwrap_or_default().to_string_lossy().into_owned();
            if name == ".pkgconfig-overlays" {
                if let Ok(overlays) = fs::read_dir(&path) {
                    let mut overlays = overlays.flatten().map(|entry| entry.path()).collect::<Vec<_>>();
                    overlays.sort();
                    for overlay in overlays {
                        add(overlay, "pkg-config overlay (regenerated on demand)");
                    }
                }
            } else if name.starts_with('.') {
                if abandoned_temporary(&name) {
                    add(path, "interrupted build's temporary output");
                }
            } else if path.is_dir()
                && !stage_ids.contains(&name)
                && !source.contains(&format!("\"{name}\""))
                && !source.contains(&format!("out/build/{name}"))
            {
                add(path, "build directory no recipe names");
            }
        }
    }
    let state = repo_root.join("out/state/stages");
    if let Ok(entries) = fs::read_dir(&state) {
        let mut entries = entries.flatten().map(|entry| entry.path()).collect::<Vec<_>>();
        entries.sort();
        for path in entries {
            let stem = path.file_stem().unwrap_or_default().to_string_lossy().into_owned();
            if path.extension().is_some_and(|extension| extension == "json") && !stage_ids.contains(&stem) {
                add(path, "manifest of a stage that no longer exists");
            }
        }
    }
    let inventory = repo_root.join("out/packages/inventory.toml");
    if let Ok(text) = fs::read_to_string(&inventory) {
        let mut artifacts = BTreeSet::new();
        let mut names = BTreeSet::new();
        for line in text.lines() {
            if let Some(value) = line.strip_prefix("artifact_path = ") {
                artifacts.insert(repo_root.join(value.trim().trim_matches('"')));
            } else if let Some(value) = line.strip_prefix("name = ") {
                names.insert(value.trim().trim_matches('"').to_string());
            }
        }
        if !artifacts.is_empty() {
            if let Ok(entries) = fs::read_dir(repo_root.join("out/packages/amd64")) {
                let mut entries = entries.flatten().map(|entry| entry.path()).collect::<Vec<_>>();
                entries.sort();
                for path in entries {
                    if path.extension().is_some_and(|extension| extension == "deb") && !artifacts.contains(&path) {
                        add(path, "package file the inventory no longer references");
                    }
                }
            }
            if let Ok(entries) = fs::read_dir(repo_root.join("out/packages/staging")) {
                let mut entries = entries.flatten().map(|entry| entry.path()).collect::<Vec<_>>();
                entries.sort();
                for path in entries {
                    let name = path.file_name().unwrap_or_default().to_string_lossy().into_owned();
                    if path.is_dir() && !names.contains(&name) {
                        add(path, "staging for a package that no longer exists");
                    }
                }
            }
        }
    }
    if let Ok(entries) = fs::read_dir(repo_root.join("out/tmp")) {
        let now = std::time::SystemTime::now();
        let mut entries = entries.flatten().map(|entry| entry.path()).collect::<Vec<_>>();
        entries.sort();
        for path in entries {
            let old = fs::symlink_metadata(&path)
                .and_then(|metadata| metadata.modified())
                .ok()
                .and_then(|modified| now.duration_since(modified).ok())
                .is_some_and(|age| age > DISK_TMP_MAX_AGE);
            if old {
                add(path, "scratch older than a day");
            }
        }
    }
    Ok(found)
}

fn current_stage_ids() -> BTreeSet<String> {
    crate::stage_graph::all_build_stages()
        .iter()
        .map(|stage| crate::stage_graph::stage_id(*stage).to_string())
        .collect()
}

fn disk_report(repo_root: &Path) -> Result<()> {
    let out = repo_root.join("out");
    println!("Disk usage under {}:", out.display());
    let mut areas = Vec::new();
    if let Ok(entries) = fs::read_dir(&out) {
        for entry in entries.flatten() {
            areas.push((disk_usage(&entry.path()), entry.path()));
        }
    }
    areas.push((disk_usage(&repo_root.join("target")), repo_root.join("target")));
    areas.sort_by(|a, b| b.0.cmp(&a.0));
    for (bytes, path) in &areas {
        let relative = path.strip_prefix(repo_root).unwrap_or(path);
        println!("  {:>10}  {}", human_bytes(*bytes), relative.display());
    }
    println!("Kept by prune (remove by hand if unwanted):");
    for relative in ["out/images", "out/qemu", "out/cache"] {
        if let Ok(entries) = fs::read_dir(repo_root.join(relative)) {
            let mut entries = entries.flatten().map(|entry| entry.path()).collect::<Vec<_>>();
            entries.sort();
            for path in entries {
                let shown = path.strip_prefix(repo_root).unwrap_or(&path);
                println!("  {:>10}  {}", human_bytes(disk_usage(&path)), shown.display());
            }
        }
    }
    let reclaimable = reclaimable_paths(repo_root, &current_stage_ids(), &build_tool_source_text(repo_root)?)?;
    let mut by_reason: BTreeMap<&str, (usize, u64)> = BTreeMap::new();
    for item in &reclaimable {
        let entry = by_reason.entry(item.reason).or_default();
        entry.0 += 1;
        entry.1 += item.bytes;
    }
    let total = reclaimable.iter().map(|item| item.bytes).sum::<u64>();
    println!("Reclaimable with `disk prune` ({}):", human_bytes(total));
    for (reason, (count, bytes)) in by_reason {
        println!("  {:>10}  {count} x {reason}", human_bytes(bytes));
    }
    for item in reclaimable.iter().filter(|item| item.bytes >= 100 * 1024 * 1024) {
        let shown = item.path.strip_prefix(repo_root).unwrap_or(&item.path);
        println!("  {:>10}  {}", human_bytes(item.bytes), shown.display());
    }
    Ok(())
}

/// Another process that writes below out/ (a `mattos-build` build, package
/// or image command; a third-party recipe build; a QEMU test; a publication)
/// and could be using the paths prune would remove.
fn running_build_processes() -> Vec<u32> {
    let own = std::process::id();
    let mut running = Vec::new();
    let Ok(entries) = fs::read_dir("/proc") else {
        return running;
    };
    for entry in entries.flatten() {
        let Ok(pid) = entry.file_name().to_string_lossy().parse::<u32>() else {
            continue;
        };
        if pid == own {
            continue;
        }
        let Ok(cmdline) = fs::read(entry.path().join("cmdline")) else {
            continue;
        };
        let args = cmdline
            .split(|byte| *byte == 0)
            .map(|arg| String::from_utf8_lossy(arg).into_owned())
            .collect::<Vec<_>>();
        let is_build_tool = args
            .first()
            .is_some_and(|program| Path::new(program).file_name().is_some_and(|name| name == "mattos-build"));
        let builds = is_build_tool
            && args.iter().skip(1).any(|arg| matches!(arg.as_str(), "build" | "package" | "image"));
        if builds || is_out_writing_script(&args) {
            running.push(pid);
        }
    }
    running
}

/// The repository scripts that build or publish below out/.
fn is_out_writing_script(args: &[String]) -> bool {
    args.iter().take(3).any(|arg| {
        arg.contains("third-party-packages/")
            || ["run_qemu.py", "BuildAndUploadThirdPartyPackages.py", "PublishPackages.py"]
                .iter()
                .any(|script| arg.ends_with(script))
    })
}

fn disk_prune(repo_root: &Path, dry_run: bool) -> Result<()> {
    let running = running_build_processes();
    if !running.is_empty() && !dry_run {
        bail!("refusing to prune while mattos-build is building (pids {running:?})");
    }
    let reclaimable = reclaimable_paths(repo_root, &current_stage_ids(), &build_tool_source_text(repo_root)?)?;
    let total = reclaimable.iter().map(|item| item.bytes).sum::<u64>();
    for item in &reclaimable {
        let shown = item.path.strip_prefix(repo_root).unwrap_or(&item.path);
        if dry_run {
            println!("would remove {:>10}  {}  ({})", human_bytes(item.bytes), shown.display(), item.reason);
        } else {
            remove_path_if_exists(&item.path)?;
        }
    }
    if dry_run {
        println!("dry run: {} path(s), {} reclaimable", reclaimable.len(), human_bytes(total));
    } else {
        println!("pruned {} path(s), {} reclaimed", reclaimable.len(), human_bytes(total));
    }
    Ok(())
}

#[cfg(test)]
mod disk_tests {
    use super::*;

    #[test]
    fn prune_selects_only_leftovers_never_current_outputs() {
        let root = tempfile::tempdir().unwrap();
        let repo = root.path();
        let write = |relative: &str, contents: &str| {
            let path = repo.join(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, contents).unwrap();
        };
        write("out/build/zlib/install/usr/lib/libz.so.1", "stage output");
        write("out/build/libx11/install/usr/lib/libX11.so.6", "auxiliary sub-build");
        write("out/build/firefox-sideload/payload", "nothing names this");
        write("out/build/.pkgconfig-overlays/0123/zlib/lib/zlib.pc", "overlay");
        write("out/build/.rootfs.building-4294967/file", "interrupted");
        write("out/state/stages/zlib.json", "{}");
        write("out/state/stages/kcalendarcore.json", "{}");
        write(
            "out/packages/inventory.toml",
            "[[package]]\nname = \"zlib1g\"\nartifact_path = \"out/packages/amd64/zlib1g_1-1mattos2_amd64.deb\"\n",
        );
        write("out/packages/amd64/zlib1g_1-1mattos2_amd64.deb", "current");
        write("out/packages/amd64/zlib1g_1-1mattos1_amd64.deb", "old");
        write("out/packages/staging/zlib1g/DEBIAN/control", "current");
        write("out/packages/staging/libmpvqt1/DEBIAN/control", "renamed");
        write("out/tmp/fresh-scratch", "recent");
        let stage_ids = BTreeSet::from(["zlib".to_string()]);
        let source = "build_xorg_autotools_component(repo_root, \"libx11\", &[], &[], &[])";
        let found = reclaimable_paths(repo, &stage_ids, source).unwrap();
        let mut removed = found
            .iter()
            .map(|item| item.path.strip_prefix(repo).unwrap().display().to_string())
            .collect::<Vec<_>>();
        removed.sort();
        assert_eq!(
            removed,
            [
                "out/build/.pkgconfig-overlays/0123",
                "out/build/.rootfs.building-4294967",
                "out/build/firefox-sideload",
                "out/packages/amd64/zlib1g_1-1mattos1_amd64.deb",
                "out/packages/staging/libmpvqt1",
                "out/state/stages/kcalendarcore.json",
            ]
        );
    }

    #[test]
    fn third_party_builds_qemu_tests_and_publication_block_pruning() {
        let args = |values: &[&str]| values.iter().map(|value| value.to_string()).collect::<Vec<_>>();
        assert!(is_out_writing_script(&args(&["python3", "third-party-packages/codex.py", "update"])));
        assert!(is_out_writing_script(&args(&["python3", "DevUtils/run_qemu.py", "--install"])));
        assert!(is_out_writing_script(&args(&["python3", "DevUtils/PublishPackages.py"])));
        assert!(!is_out_writing_script(&args(&["python3", "DevUtils/RunWiki.py"])));
    }

    #[test]
    fn temporaries_of_running_processes_are_kept() {
        assert!(!abandoned_temporary(&format!(".iso.building-{}", std::process::id())));
        assert!(!abandoned_temporary(".not-a-temporary"));
        assert!(abandoned_temporary(".rootfs.building-4294967"));
    }
}
