use super::*;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct PackageCacheManifest {
    pub(super) schema_version: u32,
    pub(super) package: String,
    pub(super) cache_key: String,
    pub(super) definition_digest: String,
    pub(super) payload_source_digest: String,
    #[serde(default)]
    pub(super) payload_configuration_digest: String,
    pub(super) dependency_digest: String,
    pub(super) payload_inventory_digest: String,
    pub(super) artifact_sha256: String,
    pub(super) artifact_path: String,
    pub(super) inventory_entry: PackageInventoryEntry,
}

#[derive(Clone, Debug)]
pub(crate) struct PackageCacheInput {
    pub(super) cache_key: String,
    pub(super) definition_digest: String,
    pub(super) payload_source_digest: String,
    pub(super) payload_configuration_digest: String,
    pub(super) dependency_digest: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct PackageSetEntry {
    pub(super) package: String,
    pub(super) cache_key: String,
    pub(super) artifact_path: String,
    pub(super) artifact_sha256: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct PackageSetManifest {
    pub(super) schema_version: u32,
    pub(super) policy: String,
    pub(super) packages: Vec<PackageSetEntry>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct PackageFacts {
    pub(super) schema_version: u32,
    pub(super) artifact_sha256: String,
    pub(super) package: String,
    pub(super) version: String,
    pub(super) architecture: String,
    pub(super) control: BTreeMap<String, String>,
    pub(super) conffiles: Vec<String>,
    pub(super) payload: Vec<PackagePayloadFact>,
    pub(super) elf_members: Vec<PackageElfMember>,
    pub(super) dependencies: Vec<String>,
    pub(super) installed_size_kib: u64,
    pub(super) provenance: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct PackagePayloadFact {
    pub(super) path: String,
    pub(super) kind: String,
    pub(super) mode: u32,
    pub(super) symlink_target: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct PackageElfMember {
    pub(super) path: String,
    pub(super) content_sha256: String,
    pub(super) soname: Option<String>,
    pub(super) needed: Vec<String>,
}

pub(crate) const PACKAGE_CACHE_SCHEMA_VERSION: u32 = 1;
pub(crate) const PACKAGE_SET_SCHEMA_VERSION: u32 = 1;

pub(crate) fn package_set_manifest_path(repo_root: &Path) -> PathBuf {
    repo_root.join("out/state/packages/package-set.json")
}

pub(crate) fn package_set_policy() -> &'static str {
    "package-set-v1:approved-package-cache-manifests-and-artifact-sha256"
}

pub(crate) fn package_set_entries(repo_root: &Path) -> Result<Vec<PackageSetEntry>> {
    let specs = package_specs();
    let mut source_digests = BTreeMap::new();
    let mut entries = Vec::with_capacity(PACKAGE_NAMES.len());
    for name in PACKAGE_NAMES {
        let spec = specs
            .iter()
            .find(|spec| spec.name == *name)
            .ok_or_else(|| anyhow!("unknown MattOS package {name}"))?;
        let version = package_version(repo_root, spec)?;
        let input = package_cache_input(repo_root, spec, &version, &mut source_digests)?;
        let manifest_path = package_cache_manifest_path(repo_root, name);
        let manifest: PackageCacheManifest = serde_json::from_slice(
            &fs::read(&manifest_path).with_context(|| {
                format!("package cache manifest missing: {}", manifest_path.display())
            })?,
        )
        .with_context(|| format!("package cache manifest is invalid: {}", manifest_path.display()))?;
        if manifest.schema_version != PACKAGE_CACHE_SCHEMA_VERSION
            || manifest.package != *name
            || manifest.cache_key != input.cache_key
        {
            bail!("package cache manifest is stale for {name}");
        }
        let artifact = repo_root.join(&manifest.artifact_path);
        if !artifact.is_file() {
            bail!("cached package artifact is missing for {name}");
        }
        let artifact_sha256 = sha256_file(&artifact)?;
        if artifact_sha256 != manifest.artifact_sha256
            || manifest.inventory_entry.sha256 != artifact_sha256
            || manifest.inventory_entry.version != version
            || manifest.inventory_entry.architecture != ARCH
        {
            bail!("cached package artifact or metadata is stale for {name}");
        }
        entries.push(PackageSetEntry {
            package: name.to_string(),
            cache_key: input.cache_key,
            artifact_path: manifest.artifact_path,
            artifact_sha256,
        });
    }
    Ok(entries)
}

pub(crate) fn write_package_set_manifest(repo_root: &Path) -> Result<()> {
    let manifest = PackageSetManifest {
        schema_version: PACKAGE_SET_SCHEMA_VERSION,
        policy: package_set_policy().to_string(),
        packages: package_set_entries(repo_root)?,
    };
    performance::atomic_write_json(&package_set_manifest_path(repo_root), &manifest)
}

/// Establishes the cheap package publication boundary used by Rootfs. A hit
/// reads package manifests and hashes only published .debs; it never walks a
/// staging tree or invokes dpkg-deb. A miss deliberately falls through to the
/// existing deep package validation/build path.
pub(crate) fn ensure_package_set(repo_root: &Path) -> Result<bool> {
    let current_entries = package_set_entries(repo_root).ok();
    let reusable = fs::read(package_set_manifest_path(repo_root))
        .ok()
        .and_then(|body| serde_json::from_slice::<PackageSetManifest>(&body).ok())
        .is_some_and(|manifest| {
            current_entries.as_ref().is_some_and(|entries| {
                manifest.schema_version == PACKAGE_SET_SCHEMA_VERSION
                    && manifest.policy == package_set_policy()
                    && entries == &manifest.packages
            })
        });
    if reusable {
        performance::timed(
            "package-set-preflight",
            "hit",
            "package cache manifests and published artifact checksums matched",
            package_set_policy(),
            || Ok(()),
        )?;
        return Ok(true);
    }
    performance::timed(
        "package-set-preflight",
        "miss",
        "package publication boundary missing, stale, or corrupt; deep package validation required",
        package_set_policy(),
        || Ok(()),
    )?;
    build_all_packages(repo_root)?;
    Ok(false)
}

pub(crate) fn ensure_package_facts(repo_root: &Path, inventory: &PackageInventory) -> Result<()> {
    for entry in &inventory.package {
        let path = repo_root
            .join("out/state/package-facts")
            .join(format!("{}.json", entry.sha256));
        let reusable = fs::read(&path)
            .ok()
            .and_then(|body| serde_json::from_slice::<PackageFacts>(&body).ok())
            .is_some_and(|facts| {
                facts.schema_version == 1
                    && facts.artifact_sha256 == entry.sha256
                    && facts.package == entry.name
            });
        if reusable {
            continue;
        }
        let staging = repo_root.join("out/packages/staging").join(&entry.name);
        let control_body = fs::read_to_string(staging.join("DEBIAN/control"))?;
        let control = repository::parse_control_paragraphs(&control_body)?
            .into_iter()
            .next()
            .ok_or_else(|| anyhow!("empty control metadata for {}", entry.name))?;
        let conffiles_path = staging.join("DEBIAN/conffiles");
        let conffiles = if conffiles_path.is_file() {
            fs::read_to_string(conffiles_path)?
                .lines()
                .map(str::to_string)
                .collect()
        } else {
            Vec::new()
        };
        let mut payload = Vec::new();
        let mut elf_members = Vec::new();
        walk_tree(&staging, &mut |member, metadata| {
            if member.starts_with(staging.join("DEBIAN")) {
                return Ok(());
            }
            #[cfg(unix)]
            let mode = {
                use std::os::unix::fs::PermissionsExt;
                metadata.permissions().mode() & 0o7777
            };
            #[cfg(not(unix))]
            let mode = 0;
            let relative = format!("/{}", member.strip_prefix(&staging)?.display());
            let kind = if metadata.file_type().is_symlink() {
                "symlink"
            } else if metadata.is_dir() {
                "directory"
            } else {
                "file"
            };
            payload.push(PackagePayloadFact {
                path: relative.clone(),
                kind: kind.into(),
                mode,
                symlink_target: if metadata.file_type().is_symlink() {
                    Some(fs::read_link(member)?.display().to_string())
                } else {
                    None
                },
            });
            if metadata.is_file() {
                if let Some(facts) = elf_cache::inspect(repo_root, member)? {
                    elf_members.push(PackageElfMember {
                        path: relative,
                        content_sha256: facts.content_sha256,
                        soname: facts.soname,
                        needed: facts.needed,
                    });
                }
            }
            Ok(())
        })?;
        payload.sort_by(|a, b| a.path.cmp(&b.path));
        elf_members.sort_by(|a, b| a.path.cmp(&b.path));
        let facts = PackageFacts {
            schema_version: 1,
            artifact_sha256: entry.sha256.clone(),
            package: entry.name.clone(),
            version: entry.version.clone(),
            architecture: entry.architecture.clone(),
            control,
            conffiles,
            payload,
            elf_members,
            dependencies: entry.dependencies.clone(),
            installed_size_kib: installed_size_kib(&staging)?,
            provenance: entry.source_component.clone(),
        };
        performance::atomic_write_json(&path, &facts)?;
    }
    Ok(())
}

pub(crate) fn package_facts_status(repo_root: &Path) -> Result<String> {
    let root = repo_root.join("out/state/package-facts");
    let count = if root.is_dir() {
        fs::read_dir(root)?.count()
    } else {
        0
    };
    Ok(format!(
        "package-audit: {count} content-addressed package fact record(s)"
    ))
}

pub(crate) fn invalidate_package_facts(repo_root: &Path) -> Result<usize> {
    let mut count = 0;
    for root in [
        repo_root.join("out/state/package-facts"),
        repo_root.join("out/state/audits"),
    ] {
        if root.is_dir() {
            count += fs::read_dir(&root)?.count();
            fs::remove_dir_all(root)?;
        }
    }
    Ok(count)
}

pub(crate) fn package_cache_manifest_path(repo_root: &Path, package: &str) -> PathBuf {
    repo_root
        .join("out/state/packages")
        .join(format!("{package}.json"))
}

pub(crate) fn package_definition_digest(spec: &PackageSpec) -> Result<String> {
    let revision = package_recipe_revision(spec.name);
    if revision == 1 {
        // Preserve the established revision-1 key exactly. Adding a recipe
        // discriminator for one package must not create a one-time rebuild of
        // every unrelated package.
        performance::digest_value(&(
            PACKAGE_CACHE_SCHEMA_VERSION,
            spec,
            ARCH,
            REVISION,
            SOURCE_DATE_EPOCH,
            "dpkg-deb --root-owner-group -Zzstd -z19",
        ))
    } else {
        performance::digest_value(&(
            PACKAGE_CACHE_SCHEMA_VERSION,
            revision,
            spec,
            ARCH,
            REVISION,
            SOURCE_DATE_EPOCH,
            "dpkg-deb --root-owner-group -Zzstd -z19",
        ))
    }
}

pub(crate) fn package_recipe_revision(package: &str) -> u32 {
    match package {
        // Preserve the new components' upstream license texts in their native packages.
        "libnl-3-200" | "libnl-genl-3-200" | "wpasupplicant" => 2,
        // Also retain administrator menu/hook edits with dpkg conffile semantics.
        "grub-efi-amd64" => 3,
        // Versioned installed kernel assets are paired using this release file.
        "mattos-installer" => 2,
        // Revision 2 adds cfdisk to the deliberately selected base payload.
        // Keep this per-package so an unrelated staging-recipe edit does not
        // invalidate every package.
        "util-linux" => 2,
        // Revision 2 preserves Git's upstream hardlink topology through
        // package staging. Without this targeted invalidation, a cached
        // revision-1 package expands the built-in aliases into hundreds of
        // independent executable copies and can exhaust the live rootfs.
        "git" => 2,
        // Revision 2 owns the split sshd-session/sshd-auth executables that
        // OpenSSH 10.4 requires after the monitor process starts.
        "openssh-server" => 2,
        // Revision 2 owns libpanelw, which CPython's source-built
        // _curses_panel extension requires at runtime.
        "libncursesw6" => 2,
        // Revision 2 exposes the pinned bundle at OpenSSL's compiled default
        // CA file as well as Debian's canonical ca-certificates path.
        "ca-certificates" => 2,
        // Revision 2 ships the source-built quirk database required by
        // libinput at runtime.  A cache hit from the library-only recipe
        // would otherwise leave the live compositor without /usr/share/libinput.
        "libinput10" => 2,
        // Revision 2 stages Meson-generated xkeyboard-config rules from an
        // output-owned mirror.  Revision 1 copied only upstream fragments,
        // leaving the required rules/evdev runtime database absent.
        "xkb-data" => 2,
        // Revision 2 retains the complete upstream LICENSES directory and
        // top-level license notice alongside WHENCE in the binary package.
        "linux-firmware" => 2,
        // Revision 2 includes Linux-PAM's source-built vendor pam_env.conf;
        // the revision-1 cache key tracked only MattOS /etc/pam.d policy.
        "libpam-runtime" => 2,
        // Revision 5 exposes Flatpak export roots through the COSMIC session
        // XDG_DATA_DIRS, so application desktop entries and their themed icons
        // resolve. Revision 4 requires COSMIC Tweaks in the aggregate desktop
        // payload.
        // Revision 3 keeps the greeter daemon display-manager-scoped instead
        // of enabling it in every multi-user/CLI boot. Revision 2 supplied the
        // freedesktop hicolor fallback index.
        // Revision 6 ships bounded, read-only physical graphics diagnostics.
        // Revision 7 adds live startup evidence and bounded text recovery.
        "cosmic-desktop" => 7,
        "mattos-compat" => 3,
        // Revision 2 preserves fuse3's documented setuid fusermount3 helper
        // in the Flatpak payload.  The document portal invokes this helper to
        // mount each user's document filesystem; a revision-1 package loses
        // that privileged target contract even though the stage install is
        // otherwise current.
        // Flatpak now owns the target-built sandbox executables it configures
        // Meson to consume.  Bump the package recipe so an older payload that
        // lacks either executable cannot be reused.
        // Revision 4 seeds the empty default OSTree repository with the
        // signed MattOS Flathub policy. This is metadata only: no applications
        // or runtimes are embedded in the image.
        // Revision 6 includes the mandatory empty refs/heads and
        // refs/remotes namespaces in the seeded OSTree repository; libostree
        // otherwise cannot complete the first system installation.
        // Revision 8 selects MattOS's administrative `sudo` group for
        // Flatpak system-helper authorization instead of the upstream
        // `wheel` default. Revision 9 ships the target-rooted optional
        // install helper, which uses the live libflatpak runtime and writes
        // only the mounted target installation.
        "flatpak" => 9,
        // Revision 2 stops copying Flatpak-owned /usr/bin/bwrap into the
        // portal package. The portal depends on Flatpak for that runtime
        // helper, leaving a single package owner for the executable.
        "xdg-desktop-portal" => 2,
        "cosmic-edit" | "mattos-cozy" => 1,
        "libgpg-error0" | "libgcrypt20" | "libassuan9" | "libksba8" | "libnpth0" | "gpgv" => 2,
        _ => 1,
    }
}

pub(crate) fn package_cache_input(
    repo_root: &Path,
    spec: &PackageSpec,
    version: &str,
    source_digests: &mut BTreeMap<String, String>,
) -> Result<PackageCacheInput> {
    let definition_digest = package_definition_digest(spec)?;
    let (payload_source_digest, payload_configuration_digest) =
        package_payload_source_digests(repo_root, spec, source_digests)?;
    let dependency_digest = package_stage_dependency_digest(repo_root, spec.source_component)?;
    let resolved_dependencies = package_dependencies(repo_root, spec)?;
    let cache_key = performance::digest_value(&(
        PACKAGE_CACHE_SCHEMA_VERSION,
        spec.name,
        version,
        ARCH,
        &definition_digest,
        &payload_source_digest,
        &dependency_digest,
        &resolved_dependencies,
        SOURCE_DATE_EPOCH,
        "deb-format=2.0;compression=zstd;level=19;root-owner-group=true",
    ))?;
    Ok(PackageCacheInput {
        cache_key,
        definition_digest,
        payload_source_digest,
        payload_configuration_digest,
        dependency_digest,
    })
}

pub(crate) fn package_stage_dependency_digest(repo_root: &Path, source_component: &str) -> Result<String> {
    let stage_dependencies = package_stage_dependencies(source_component);
    let mut dependency_values = BTreeMap::new();
    for dependency in stage_dependencies {
        let value = match performance::read_stage_manifest(repo_root, dependency) {
            Ok(manifest) => manifest.output_content_digest,
            Err(_) => "<missing>".to_string(),
        };
        dependency_values.insert(dependency.to_string(), value);
    }
    performance::digest_value(&dependency_values)
}

pub(crate) fn package_payload_source_digests(
    repo_root: &Path,
    spec: &PackageSpec,
    source_digests: &mut BTreeMap<String, String>,
) -> Result<(String, String)> {
    let source_key = spec.source_component.to_string();
    let upstream_source_digest = if let Some(digest) = source_digests.get(&source_key) {
        digest.clone()
    } else {
        let roots = package_source_roots(spec.source_component)
            .iter()
            .map(PathBuf::from)
            .collect::<Vec<_>>();
        let digest = performance::tracked_source_digest(repo_root, &roots, false)?;
        source_digests.insert(source_key, digest.clone());
        digest
    };
    let configuration_roots = package_configuration_roots(spec.name)
        .iter()
        .map(PathBuf::from)
        .collect::<Vec<_>>();
    let payload_configuration_digest = if configuration_roots.is_empty() {
        String::new()
    } else {
        performance::tracked_source_digest(repo_root, &configuration_roots, false)?
    };
    let payload_source_digest = if payload_configuration_digest.is_empty() {
        upstream_source_digest
    } else {
        performance::digest_value(&(&upstream_source_digest, &payload_configuration_digest))?
    };
    Ok((payload_source_digest, payload_configuration_digest))
}

pub(crate) fn validate_package_cache(
    repo_root: &Path,
    spec: &PackageSpec,
    version: &str,
    staging: &Path,
    artifact: &Path,
    input: &PackageCacheInput,
) -> Result<PackageCacheManifest> {
    let path = package_cache_manifest_path(repo_root, spec.name);
    let manifest: PackageCacheManifest = serde_json::from_slice(
        &fs::read(&path)
            .with_context(|| format!("package cache manifest missing: {}", path.display()))?,
    )
    .with_context(|| format!("package cache manifest is invalid: {}", path.display()))?;
    if manifest.schema_version != PACKAGE_CACHE_SCHEMA_VERSION {
        bail!("package cache schema changed")
    }
    if manifest.package != spec.name || manifest.cache_key != input.cache_key {
        bail!("package cache input key changed")
    }
    if manifest.definition_digest != input.definition_digest
        || manifest.payload_source_digest != input.payload_source_digest
        || manifest.payload_configuration_digest != input.payload_configuration_digest
        || manifest.dependency_digest != input.dependency_digest
    {
        bail!("package cache component digest changed")
    }
    if !staging.is_dir() || !artifact.is_file() {
        bail!("cached package staging tree or artifact is missing")
    }
    let payload_digest = performance::measure_package_validation_step("payload_inventory", || {
        performance::output_path_digest(repo_root, staging)
    })?;
    if payload_digest != manifest.payload_inventory_digest {
        bail!("cached package payload inventory/content/modes changed")
    }
    let artifact_sha =
        performance::measure_package_validation_step("artifact_sha256", || sha256_file(artifact))?;
    if artifact_sha != manifest.artifact_sha256 {
        bail!("cached package artifact checksum changed")
    }
    performance::measure_package_validation_step("dpkg_deb", || {
        verify_deb(artifact, spec.name, version)
    })?;
    if manifest.inventory_entry.name != spec.name
        || manifest.inventory_entry.version != version
        || manifest.inventory_entry.architecture != ARCH
        || manifest.inventory_entry.sha256 != artifact_sha
        || repo_root.join(&manifest.artifact_path) != artifact
    {
        bail!("cached package inventory/control metadata changed")
    }
    Ok(manifest)
}

pub(crate) fn print_package_cache_status(repo_root: &Path) -> Result<()> {
    let mut hits = 0usize;
    for name in PACKAGE_NAMES {
        let path = package_cache_manifest_path(repo_root, name);
        if path.is_file() {
            hits += 1;
            println!("package:{name}: manifest present");
        } else {
            println!("package:{name}: rebuild: manifest absent");
        }
    }
    println!("package cache manifests: {hits}/{}", PACKAGE_NAMES.len());
    Ok(())
}

pub(crate) fn explain_package_cache(repo_root: &Path, name: &str) -> Result<()> {
    validate_package_name(name)?;
    let spec = package_specs()
        .into_iter()
        .find(|spec| spec.name == name)
        .ok_or_else(|| anyhow!("unknown MattOS package {name}"))?;
    let version = package_version(repo_root, &spec)?;
    let mut source_digests = BTreeMap::new();
    let input = package_cache_input(repo_root, &spec, &version, &mut source_digests)?;
    let staging = repo_root.join("out/packages/staging").join(name);
    let artifact = repo_root
        .join("out/packages/amd64")
        .join(format!("{name}_{version}_{ARCH}.deb"));
    match validate_package_cache(repo_root, &spec, &version, &staging, &artifact, &input) {
        Ok(_) => println!("package:{name}: reusable; key={}", input.cache_key),
        Err(error) => println!("package:{name}: rebuild: {error:#}"),
    }
    Ok(())
}

pub(crate) fn invalidate_package_cache(repo_root: &Path, name: &str) -> Result<()> {
    validate_package_name(name)?;
    if !PACKAGE_NAMES.contains(&name) {
        bail!("unknown MattOS package {name}")
    }
    let path = package_cache_manifest_path(repo_root, name);
    if path.exists() {
        fs::remove_file(&path)?;
        println!("invalidated package cache manifest: {name}");
    } else {
        println!("package cache manifest was already absent: {name}");
    }
    println!(
        "staging and .deb outputs were preserved; the next package build will validate/rebuild them"
    );
    Ok(())
}
