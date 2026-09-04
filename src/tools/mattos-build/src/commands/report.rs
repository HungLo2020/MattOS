fn build_image(repo_root: &Path) -> Result<()> {
    build_rootfs(repo_root)?;
    build_live_root(repo_root)?;
    build_initramfs(repo_root)?;
    build_iso(repo_root)
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ArtifactRecord {
    role: &'static str,
    path: String,
    bytes: u64,
    sha256: String,
    detail: &'static str,
}

fn reject_obsolete_full_root_initramfs(repo_root: &Path) -> Result<()> {
    let stale = OBSOLETE_FULL_ROOT_INITRAMFS_PATHS
        .iter()
        .filter(|path| repo_root.join(path).exists())
        .copied()
        .collect::<Vec<_>>();
    if !stale.is_empty() {
        bail!(
            "obsolete full-root initramfs artifact(s) coexist with the live-root architecture: {}; remove these generated outputs and use {} plus {}",
            stale.join(", "),
            INITRAMFS_ARCHIVE_PATH,
            LIVE_ROOT_IMAGE_PATH,
        );
    }
    Ok(())
}

fn artifact_record(
    repo_root: &Path,
    role: &'static str,
    relative: &str,
    detail: &'static str,
) -> Result<ArtifactRecord> {
    let path = repo_root.join(relative);
    let metadata =
        fs::metadata(&path).with_context(|| format!("{role} is missing at {}", path.display()))?;
    if !metadata.is_file() {
        bail!("{role} is not a regular file: {}", path.display());
    }
    Ok(ArtifactRecord {
        role,
        path: relative.to_string(),
        bytes: metadata.len(),
        sha256: performance::sha256_file(&path)?,
        detail,
    })
}

fn extract_efi_image_record(repo_root: &Path) -> Result<ArtifactRecord> {
    let iso = repo_root.join(FINAL_ISO_PATH);
    let temporary = repo_root
        .join("out/tmp")
        .join(format!("artifact-report-efi-{}.img", std::process::id()));
    if let Some(parent) = temporary.parent() {
        fs::create_dir_all(parent)?;
    }
    remove_path_if_exists(&temporary)?;
    let result = run_cmd(
        repo_root,
        "xorriso",
        &[
            "-osirrox",
            "on",
            "-indev",
            path_str(&iso)?,
            "-extract",
            "/efi.img",
            path_str(&temporary)?,
        ],
    );
    if let Err(error) = result {
        let _ = remove_path_if_exists(&temporary);
        return Err(error).context("failed to extract the UEFI image from the final ISO");
    }
    let record = ArtifactRecord {
        role: "UEFI ISO boot image",
        path: format!("{FINAL_ISO_PATH}:/efi.img"),
        bytes: fs::metadata(&temporary)?.len(),
        sha256: performance::sha256_file(&temporary)?,
        detail: "FAT image inside ISO",
    };
    remove_path_if_exists(&temporary)?;
    Ok(record)
}

fn collect_artifact_records(repo_root: &Path) -> Result<Vec<ArtifactRecord>> {
    reject_obsolete_full_root_initramfs(repo_root)?;
    validate_early_initramfs(&repo_root.join(INITRAMFS_ARCHIVE_PATH))?;
    validate_squashfs_image(&repo_root.join(LIVE_ROOT_IMAGE_PATH))?;
    validate_staged_grub_config(&repo_root.join("out/build/iso/boot/grub/grub.cfg"))?;

    let expanded = Command::new("xz")
        .args(["-dc"])
        .arg(repo_root.join(INITRAMFS_ARCHIVE_PATH))
        .output()
        .context("failed to expand the live early initramfs for reporting")?;
    if !expanded.status.success() {
        bail!("xz rejected the live early initramfs");
    }

    let mut records = vec![
        artifact_record(
            repo_root,
            "Kernel",
            "out/build/linux/build/arch/x86/boot/bzImage",
            "Linux bzImage",
        )?,
        artifact_record(
            repo_root,
            "Live early initramfs",
            INITRAMFS_ARCHIVE_PATH,
            "XZ newc; minimal /init only",
        )?,
        ArtifactRecord {
            role: "Live early initramfs (uncompressed)",
            path: INITRAMFS_ARCHIVE_PATH.to_string(),
            bytes: expanded.stdout.len() as u64,
            sha256: format!("{:x}", Sha256Hasher::digest(&expanded.stdout)),
            detail: "uncompressed newc stream",
        },
        artifact_record(
            repo_root,
            "Live root SquashFS",
            LIVE_ROOT_IMAGE_PATH,
            "read-only live root (Zstd level 12)",
        )?,
        artifact_record(
            repo_root,
            "Installed initramfs",
            INSTALLED_INITRAMFS_PATH,
            "XZ newc for installed Btrfs root",
        )?,
    ];
    records.push(extract_efi_image_record(repo_root)?);
    records.push(artifact_record(
        repo_root,
        "Final ISO",
        FINAL_ISO_PATH,
        "hybrid BIOS/UEFI ISO",
    )?);
    Ok(records)
}

fn report_artifacts(repo_root: &Path) -> Result<()> {
    let records = collect_artifact_records(repo_root)?;
    let mut report = String::from("role\tpath\tbytes\tsha256\tdetail\n");
    for record in &records {
        report.push_str(&format!(
            "{}\t{}\t{}\t{}\t{}\n",
            record.role, record.path, record.bytes, record.sha256, record.detail
        ));
        println!(
            "{:<38} {:>12} bytes  {}  {}",
            format!("{}:", record.role),
            record.bytes,
            record.sha256,
            record.path
        );
    }
    let destination = repo_root.join(ARTIFACT_REPORT_PATH);
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&destination, report)?;
    println!("Artifact report: {ARTIFACT_REPORT_PATH}");
    Ok(())
}
