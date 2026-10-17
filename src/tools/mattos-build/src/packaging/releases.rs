use super::*;

pub(crate) fn release_version(repo: &Path) -> Result<String> {
    let value: toml::Value = toml::from_str(&fs::read_to_string(
        repo.join("src/system/release/current.toml"),
    )?)?;
    if value
        .get("schema_version")
        .and_then(toml::Value::as_integer)
        != Some(1)
    {
        bail!("unsupported release selector schema");
    }
    let version = value["version"]
        .as_str()
        .context("release version missing")?
        .to_string();
    let fields: Vec<_> = version.split('.').collect();
    if fields.len() != 3
        || fields[0].len() != 2
        || fields[1].len() != 2
        || !fields
            .iter()
            .all(|f| !f.is_empty() && f.bytes().all(|b| b.is_ascii_digit()))
        || !fields[1].parse::<u8>().is_ok_and(|m| (1..=12).contains(&m))
        || fields[2].starts_with('0')
        || !fields[2].parse::<u32>().is_ok_and(|r| r > 0)
    {
        bail!("release version must be YY.MM.revision, for example 26.10.1");
    }
    Ok(version)
}

pub(crate) fn write_release_lock(repo: &Path) -> Result<()> {
    let version = release_version(repo)?;
    let packages: BTreeMap<_, _> = package_specs()
        .iter()
        .map(|spec| Ok((spec.name.to_string(), package_version(repo, spec)?)))
        .collect::<Result<_>>()?;
    let mut migrations = Vec::new();
    let migration_root = repo.join("src/system/release/migrations");
    fn collect(root: &Path, path: &Path, entries: &mut Vec<PathBuf>) -> Result<()> {
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                collect(root, &entry.path(), entries)?;
            } else if entry.file_type()?.is_file() {
                entries.push(entry.path().strip_prefix(root)?.into());
            }
        }
        Ok(())
    }
    let mut paths = Vec::new();
    collect(&migration_root, &migration_root, &mut paths)?;
    paths.sort_by_key(|path| {
        let label = path
            .components()
            .next()
            .unwrap()
            .as_os_str()
            .to_string_lossy();
        let version: Vec<u32> = label.split('.').map(|n| n.parse().unwrap_or(0)).collect();
        (version, path.clone())
    });
    for path in paths {
        migrations.push(serde_json::json!({"id": path.to_string_lossy(), "path": format!("usr/lib/mattos/migrations/{}", path.display()), "sha256": sha256_file(&migration_root.join(path))?}));
    }
    let manifest = serde_json::json!({"schema_version":1,"version":version,"packages":packages,"migrations":migrations});
    let path = repo
        .join("src/system/release/manifests")
        .join(format!("{version}.json"));
    fs::create_dir_all(path.parent().unwrap())?;
    fs::write(
        &path,
        format!("{}\n", serde_json::to_string_pretty(&manifest)?),
    )?;
    println!("Release package versions locked in {}", path.display());
    Ok(())
}

pub(super) fn stage_release(repo: &Path, staging: &Path) -> Result<()> {
    let version = release_version(repo)?;
    let path = repo
        .join("src/system/release/manifests")
        .join(format!("{version}.json"));
    let manifest: serde_json::Value = serde_json::from_slice(
        &fs::read(&path).context("run mattos-build release-lock before building a release")?,
    )?;
    if manifest["version"].as_str() != Some(&version) {
        bail!("release lock version mismatch");
    }
    for spec in package_specs() {
        let expected = package_version(repo, &spec)?;
        if manifest["packages"][spec.name].as_str() != Some(&expected) {
            bail!(
                "release lock is stale for {}; prepare a new release or regenerate an unpublished lock",
                spec.name
            );
        }
    }
    staging::copy_path_preserving(&path, &staging.join("usr/share/mattos/release.json"))?;
    fs::create_dir_all(staging.join("usr/lib"))?;
    fs::write(
        staging.join("usr/lib/os-release"),
        format!(
            "NAME=\"MattOS\"\nID=mattos\nPRETTY_NAME=\"MattOS {version}\"\nVERSION=\"{version}\"\nVERSION_ID=\"{version}\"\n"
        ),
    )?;
    fs::create_dir_all(staging.join("etc"))?;
    std::os::unix::fs::symlink("../usr/lib/os-release", staging.join("etc/os-release"))?;
    copy_tree_preserving(
        &repo.join("src/system/release/migrations"),
        &staging.join("usr/lib/mattos/migrations"),
    )?;
    fs::create_dir_all(staging.join("usr/lib/systemd/system"))?;
    fs::write(
        staging.join("usr/lib/systemd/system/mattos-release-migrations.service"),
        "[Unit]\nDescription=Apply pending MattOS release migrations\nAfter=local-fs.target\nBefore=multi-user.target\n[Service]\nType=oneshot\nExecStart=/usr/bin/mattos-upgrade migrate\nRemainAfterExit=yes\n[Install]\nWantedBy=multi-user.target\n",
    )?;
    fs::create_dir_all(staging.join("etc/systemd/system/multi-user.target.wants"))?;
    std::os::unix::fs::symlink(
        "/usr/lib/systemd/system/mattos-release-migrations.service",
        staging
            .join("etc/systemd/system/multi-user.target.wants/mattos-release-migrations.service"),
    )?;
    fs::write(
        staging.join("DEBIAN/postinst"),
        "#!/bin/sh\nset -e\n[ \"$1\" = configure ] || exit 0\n[ -z \"${DPKG_ROOT:-}\" ] || exit 0\n# The release coordinator runs and journals system migrations after APT completes.\n# Direct APT users can run mattos-upgrade migrate explicitly.\nexit 0\n",
    )?;
    set_mode(staging.join("DEBIAN/postinst"), 0o755)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn release_numbering_rejects_ambiguous_or_invalid_versions() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("src/system/release/current.toml");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        for version in ["26.10.1", "26.10.12", "27.01.1"] {
            fs::write(&path, format!("schema_version=1\nversion='{version}'\n")).unwrap();
            assert_eq!(release_version(root.path()).unwrap(), version);
        }
        for version in [
            "26.00.1",
            "26.13.1",
            "2026.10.1",
            "26.1.1",
            "26.10.0",
            "26.10.01",
        ] {
            fs::write(&path, format!("schema_version=1\nversion='{version}'\n")).unwrap();
            assert!(release_version(root.path()).is_err());
        }
    }
    #[test]
    fn release_data_is_not_an_upstream_or_upgrade_binary_compilation_input() {
        use crate::stage_inputs;
        let metadata = Path::new("src/system/release/current.toml");
        let lock = Path::new("src/system/release/manifests/26.10.1.json");
        for stage in crate::package_source_build_plan() {
            for input in stage_inputs::source_inputs(stage) {
                assert!(
                    !metadata.starts_with(&input),
                    "{stage:?} unnecessarily depends on release identity"
                );
                assert!(
                    !lock.starts_with(&input),
                    "{stage:?} unnecessarily depends on release package versions"
                );
            }
        }
        let specs = package_specs();
        let release = specs.iter().find(|p| p.name == "mattos-release").unwrap();
        assert_eq!(release.source_component, "mattos-release-data");
        assert!(release.depends.contains(&"mattos-upgrade"));
        assert!(release.replaces.contains(&"mattos-base-files"));
        assert!(
            !specs
                .iter()
                .find(|p| p.name == "mattos-base-files")
                .unwrap()
                .provides
                .contains(&"mattos-release")
        );
    }

    #[test]
    fn smb_archive_pins_do_not_invalidate_unrelated_toolchain_builds() {
        use crate::{stage_graph::BuildStage, stage_inputs};
        let policy = PathBuf::from("upstream/policies/smb-release-archives.toml");
        for stage in [BuildStage::Nettle, BuildStage::LibTasn1, BuildStage::GnuTls] {
            assert!(stage_inputs::source_inputs(stage).contains(&policy));
        }
        for stage in [BuildStage::Rust, BuildStage::Autoconf, BuildStage::Gzip] {
            assert!(!stage_inputs::source_inputs(stage).contains(&policy));
        }
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
        let data: toml::Value =
            toml::from_str(&fs::read_to_string(root.join(policy)).unwrap()).unwrap();
        assert_eq!(data["archive"].as_array().unwrap().len(), 3);
    }
}
