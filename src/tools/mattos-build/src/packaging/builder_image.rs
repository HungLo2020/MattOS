//! The MattOS builder container image (`mattos-build builder-image`).
//!
//! Third-party recipes build inside this image so their packages link against
//! the MattOS libraries they will run with.  The image is assembled from the
//! built MattOS packages exactly like the root filesystem (a chrootless dpkg
//! install), packed as one deterministic layer and written as an OCI image
//! archive that `podman load` / `docker load` accept.  Its tag is derived from
//! its manifest digest, and an unchanged package set reuses the archive.

use super::*;

/// What the image installs, plus the dependency closure of each.
pub(crate) const BUILDER_IMAGE_PACKAGES: &[&str] = &[
    "mattos-build-essential",
    "grep",
    "findutils",
    "diffutils",
    // ldd, used to derive a package's library dependencies.
    "libc-bin",
    "coreutils",
    "mattos-brush",
    "python3",
    "ca-certificates",
    "curl",
    "git",
    "tar",
    "gzip",
    "xz-utils",
    "bzip2",
    "zstd",
    "file",
    "patch",
    "libncurses-dev",
    "libcap-dev",
    "libnl-3-dev",
    "libnl-genl-3-dev",
    "libsystemd-dev",
    "libacl1-dev",
    "libattr1-dev",
    // Rust recipes (ripgrep, codex, netavark) build with the MattOS toolchain.
    "rustc",
    "cargo",
    // QEMU's configure builds a Python venv; SDL2 needs the EGL headers.
    "python3-venv",
    "libglvnd-dev",
    "zlib1g-dev",
    "libssl-dev",
    "libpcre2-dev",
    "libzstd-dev",
    "libglib2.0-dev",
    "libpixman-1-dev",
    "libwayland-dev",
    "wayland-protocols",
    "libxkbcommon-dev",
    // json-c's runtime package carries its development files.
    "libjson-c5",
    "perl",
    "m4",
    "autoconf",
    "automake",
    "libtool",
    "meson",
    "ninja-build",
];

const IMAGE_NAME: &str = "localhost/mattos-builder";
const IMAGE_ARCHIVE: &str = "out/images/mattos-builder.oci.tar";
const IMAGE_METADATA: &str = "out/images/mattos-builder.json";
const IMAGE_WORK: &str = "out/build/builder-image";
/// Bumped when the image assembly (not its packages) changes.
const IMAGE_FORMAT: &str = "mattos-builder-image-v2";

#[derive(Serialize, Deserialize)]
struct BuilderImageMetadata {
    reference: String,
    manifest_digest: String,
    archive: String,
    input_digest: String,
    packages: Vec<String>,
}

/// `names` and everything they depend on, in installation order.
fn package_closure(names: &[&str]) -> Result<Vec<&'static str>> {
    let specs = package_specs();
    let mut selected = BTreeSet::new();
    let mut pending = names.to_vec();
    while let Some(name) = pending.pop() {
        let spec = specs
            .iter()
            .find(|spec| spec.name == name)
            .ok_or_else(|| anyhow!("builder image package {name} is not a MattOS package"))?;
        if selected.insert(spec.name) {
            pending.extend(spec.depends.iter().copied());
        }
    }
    let closure = specs
        .into_iter()
        .filter(|spec| selected.contains(spec.name))
        .collect::<Vec<_>>();
    registry::package_install_order_for(&closure, PACKAGE_NAMES)
}

pub(crate) fn build_builder_image(repo_root: &Path) -> Result<()> {
    let inventory = read_inventory(repo_root)?;
    let order = package_closure(BUILDER_IMAGE_PACKAGES)?;
    let mut input = Sha256Hasher::new();
    input.update(IMAGE_FORMAT.as_bytes());
    for name in &order {
        let entry = inventory
            .package
            .iter()
            .find(|entry| entry.name == *name)
            .ok_or_else(|| anyhow!("package {name} is not built; run `mattos-build build all` first"))?;
        input.update(format!("\n{name}={}", entry.sha256).as_bytes());
    }
    let input_digest = format!("{:x}", input.finalize());
    let metadata_path = repo_root.join(IMAGE_METADATA);
    if let Some(existing) = fs::read_to_string(&metadata_path)
        .ok()
        .and_then(|text| serde_json::from_str::<BuilderImageMetadata>(&text).ok())
        && existing.input_digest == input_digest
        && repo_root.join(&existing.archive).is_file()
    {
        println!("builder image {} is up to date", existing.reference);
        return Ok(());
    }

    let work = repo_root.join(IMAGE_WORK);
    remove_path_if_exists(&work)?;
    let root = work.join("root");
    fs::create_dir_all(&root)?;
    install_package_set(repo_root, &root, &inventory, &order)?;
    remove_dpkg_transaction_locks(&root)?;
    // Mount points for the recipe framework, and a sticky /tmp.
    for directory in ["work", "recipes", "tmp"] {
        fs::create_dir_all(root.join(directory))?;
    }
    set_mode(root.join("tmp"), 0o1777)?;
    // The image's LANG: glibc has no built-in C.UTF-8, so compile it as the
    // root filesystem compiles en_US.UTF-8 (otherwise perl and every other
    // locale user warns and falls back to plain C).
    crate::compile_locale(repo_root, &root, "C", "UTF-8", "C.UTF-8")?;
    if !root.join("usr/lib/x86_64-linux-gnu/locale/C.utf8").is_dir() {
        bail!("builder image C.UTF-8 locale generation produced no compiled locale");
    }

    let layer = work.join("layer.tar");
    deterministic_tar(&root, &layer)?;
    let diff_id = format!("sha256:{}", performance::sha256_file(&layer)?);
    let layout = work.join("layout");
    let blobs = layout.join("blobs/sha256");
    fs::create_dir_all(&blobs)?;
    let compressed = work.join("layer.tar.gz");
    run_cmd(&work, "sh", &["-c", "gzip -n -6 -c layer.tar > layer.tar.gz"])?;
    remove_path_if_exists(&layer)?;
    let (layer_digest, layer_size) = store_blob(&blobs, &fs::read(&compressed)?)?;
    remove_path_if_exists(&compressed)?;

    let config = serde_json::json!({
        "architecture": "amd64",
        "os": "linux",
        "config": {
            "Env": [
                "PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin",
                "LANG=C.UTF-8",
            ],
            "Cmd": ["/usr/bin/sh"],
            "WorkingDir": "/work",
        },
        "rootfs": {"type": "layers", "diff_ids": [diff_id]},
        "history": [{"created_by": format!("mattos-build builder-image ({IMAGE_FORMAT})")}],
    });
    let (config_digest, config_size) = store_blob(&blobs, serde_json::to_vec(&config)?.as_slice())?;
    let manifest = serde_json::json!({
        "schemaVersion": 2,
        "mediaType": "application/vnd.oci.image.manifest.v1+json",
        "config": {
            "mediaType": "application/vnd.oci.image.config.v1+json",
            "digest": config_digest,
            "size": config_size,
        },
        "layers": [{
            "mediaType": "application/vnd.oci.image.layer.v1.tar+gzip",
            "digest": layer_digest,
            "size": layer_size,
        }],
    });
    let (manifest_digest, manifest_size) = store_blob(&blobs, serde_json::to_vec(&manifest)?.as_slice())?;
    let tag = &manifest_digest["sha256:".len().."sha256:".len() + 12];
    let reference = format!("{IMAGE_NAME}:{tag}");
    let index = serde_json::json!({
        "schemaVersion": 2,
        "manifests": [{
            "mediaType": "application/vnd.oci.image.manifest.v1+json",
            "digest": manifest_digest,
            "size": manifest_size,
            "annotations": {"org.opencontainers.image.ref.name": reference},
        }],
    });
    fs::write(layout.join("index.json"), serde_json::to_vec(&index)?)?;
    fs::write(layout.join("oci-layout"), br#"{"imageLayoutVersion":"1.0.0"}"#)?;
    let archive = repo_root.join(IMAGE_ARCHIVE);
    fs::create_dir_all(archive.parent().expect("image archive has a parent"))?;
    deterministic_tar(&layout, &archive)?;
    remove_path_if_exists(&work)?;
    let metadata = BuilderImageMetadata {
        reference: reference.clone(),
        manifest_digest,
        archive: IMAGE_ARCHIVE.to_string(),
        input_digest,
        packages: order.iter().map(|name| name.to_string()).collect(),
    };
    performance::atomic_write(&metadata_path, serde_json::to_string_pretty(&metadata)?.as_bytes())?;
    println!("built builder image {reference} ({} packages)", order.len());
    Ok(())
}

fn store_blob(blobs: &Path, bytes: &[u8]) -> Result<(String, usize)> {
    let digest = format!("{:x}", Sha256Hasher::digest(bytes));
    fs::write(blobs.join(&digest), bytes)?;
    Ok((format!("sha256:{digest}"), bytes.len()))
}

/// A byte-reproducible tar of `source`: sorted entries, fixed times, root
/// ownership, no access or change times.
fn deterministic_tar(source: &Path, archive: &Path) -> Result<()> {
    run_cmd(
        source,
        "tar",
        &[
            "--sort=name",
            &format!("--mtime=@{SOURCE_DATE_EPOCH}"),
            "--owner=0",
            "--group=0",
            "--numeric-owner",
            "--format=posix",
            "--pax-option=exthdr.name=%d/PaxHeaders/%f,delete=atime,delete=ctime",
            "-cf",
            path_str(archive)?,
            ".",
        ],
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builder_image_closure_is_ordered_and_holds_the_build_environment() {
        let order = package_closure(BUILDER_IMAGE_PACKAGES).unwrap();
        for required in [
            "gcc", "g++", "make", "cmake", "pkgconf", "dash", "sed", "mawk", "grep", "findutils", "diffutils",
            "python3", "dpkg", "libc6-dev",
        ] {
            assert!(order.contains(&required), "builder image lacks {required}");
        }
        // Every package follows its dependencies.
        let specs = package_specs();
        for (position, name) in order.iter().enumerate() {
            let spec = specs.iter().find(|spec| spec.name == *name).unwrap();
            for dependency in spec.depends {
                let at = order.iter().position(|candidate| candidate == dependency).unwrap();
                assert!(at < position, "{name} precedes its dependency {dependency}");
            }
        }
        // Neither the desktop nor the service policy (NetworkManager, the
        // MattOS units) is part of a build container.
        assert!(!order.contains(&"mattos-plasma"));
        assert!(!order.contains(&"mattos-base-runtime"));
        assert!(!order.contains(&"network-manager"));
    }

    #[test]
    fn deterministic_tar_ignores_times_and_owners() {
        let temp = tempfile::tempdir().unwrap();
        let tree = temp.path().join("tree");
        fs::create_dir_all(tree.join("b")).unwrap();
        fs::write(tree.join("b/file"), "payload").unwrap();
        fs::write(tree.join("a"), "first").unwrap();
        let first = temp.path().join("first.tar");
        deterministic_tar(&tree, &first).unwrap();
        filetime::set_file_mtime(tree.join("a"), filetime::FileTime::from_unix_time(1, 0)).unwrap();
        let second = temp.path().join("second.tar");
        deterministic_tar(&tree, &second).unwrap();
        assert_eq!(fs::read(first).unwrap(), fs::read(second).unwrap());
    }
}
