//! Base-system payloads: filesystem, init, package management, boot,
//! authentication, networking and firmware.

use super::*;

pub(super) fn stage_mattos_compat(repo_root: &Path, staging: &Path) -> Result<()> {
    // Build like every other target Rust component: through the MattOS
    // compiler/linker environment and sysroot, into an output-owned target
    // directory (never the developer's shared `target/` or host linker).
    let target = repo_root.join("out/build/mattos-compat/cargo-target");
    run_cmd_with_env_overrides(
        repo_root,
        "cargo",
        &["build", "--locked", "--release", "-p", "mattos-compat"],
        &[("CARGO_TARGET_DIR", target.display().to_string())],
    )
    .context("cargo failed while building mattos-compat")?;
    let binary = target.join("release/mattos-compat");
    if !binary.is_file() {
        bail!("mattos-compat build did not produce {}", binary.display());
    }
    copy_preserving(&binary, &staging.join("usr/bin/mattos-compat"))?;
    let nspawn = repo_root.join("out/build/systemd/install/usr/bin/systemd-nspawn");
    if !nspawn.is_file() {
        bail!(
            "systemd-nspawn is missing from the systemd stage at {}; enable the nspawn component before building mattos-compat",
            nspawn.display()
        );
    }
    copy_preserving(&nspawn, &staging.join("usr/bin/systemd-nspawn"))?;
    Ok(())
}

pub(super) fn stage_mattos_installer(repo_root: &Path, staging: &Path) -> Result<()> {
    let installer = repo_root.join("out/build/installer");
    stage_executable(
        &installer.join("cargo-target/release/mattos-install"),
        &staging.join("usr/bin/mattos-install"),
        0o755,
    )?;
    let assets = staging.join("usr/lib/mattos/installer");
    fs::create_dir_all(&assets)?;
    for (source, name) in [
        (
            repo_root.join("out/build/linux/build/arch/x86/boot/bzImage"),
            "vmlinuz",
        ),
        (
            repo_root.join("out/build/installed-initramfs.cpio.xz"),
            "installed-initramfs.cpio.xz",
        ),
        (installer.join("BOOTX64.EFI"), "BOOTX64.EFI"),
        (
            repo_root.join("out/build/linux/kernel-release"),
            "kernel-release",
        ),
    ] {
        copy_preserving(&source, &assets.join(name))?;
    }
    copy_preserving(
        &repo_root.join("src/system/installer/policy/example-plan.toml"),
        &staging.join("usr/share/doc/mattos-installer/example-plan.toml"),
    )?;
    copy_preserving(
        &repo_root.join("src/system/installer/PROVENANCE.md"),
        &staging.join("usr/share/doc/mattos-installer/PROVENANCE.md"),
    )?;
    for name in ["mattos-install-cli.service", "mattos-install-cli.target"] {
        copy_preserving(
            &repo_root.join("src/system/units").join(name),
            &staging.join("usr/lib/systemd/system").join(name),
        )?;
    }
    Ok(())
}

pub(super) fn stage_filesystem(staging: &Path) -> Result<()> {
    for rel in [
        "usr/bin",
        "usr/sbin",
        "usr/lib",
        "usr/lib64",
        "usr/share",
        "usr/share/doc",
        "etc",
        "var",
        "var/lib",
        "home",
        "root",
        "run",
        "tmp",
    ] {
        fs::create_dir_all(staging.join(rel))?;
    }
    set_mode(staging.join("root"), 0o700)?;
    set_mode(staging.join("tmp"), 0o1777)?;
    #[cfg(unix)]
    for (link, target) in [
        ("bin", "usr/bin"),
        ("sbin", "usr/sbin"),
        ("lib", "usr/lib"),
        ("lib64", "usr/lib64"),
    ] {
        std::os::unix::fs::symlink(target, staging.join(link))?;
    }
    Ok(())
}

/// Ship the pinned ISO-codes JSON contract required by locales-rs.  This is
/// source data, not a host locale database and is intentionally limited to
/// the three registries consumed by MattOS locale selection.
pub(crate) fn stage_iso_codes(repo_root: &Path, staging: &Path) -> Result<()> {
    let source = repo_root.join("src/system/data/iso-codes");
    let destination = staging.join("usr/share/iso-codes/json");
    fs::create_dir_all(&destination)?;
    for name in ["iso_3166-1.json", "iso_639-2.json", "iso_639-3.json"] {
        let path = source.join(name);
        if !path.is_file() {
            bail!("ISO-codes source is missing {name}");
        }
        copy_preserving(&path, &destination.join(name))?;
    }
    copy_preserving(
        &source.join("PROVENANCE.md"),
        &staging.join("usr/share/doc/iso-codes/PROVENANCE.md"),
    )?;
    Ok(())
}

/// Compile the pinned IANA database in an output-owned mirror and package
/// only the runtime zoneinfo tree; no host timezone files are consulted.
pub(super) fn stage_tzdata(repo_root: &Path, staging: &Path) -> Result<()> {
    let source = repo_root.join("src/system/data/tzdata");
    let output = repo_root.join("out/build/tzdata");
    let build_source = output.join("source");
    let zoneinfo = output.join("zoneinfo");
    remove_path_if_exists(&output)?;
    sync_build_source(&source, &build_source)?;
    // LICENSE files are globally ignored in the MattOS source tree, while
    // tzcode's Makefile still lists one as a version-generation dependency.
    // Keep the authoritative import untouched and provide the source README
    // as the disposable build-tree notice instead; it records the upstream
    // public-domain/BSD licensing terms and is also suitable package notice
    // material when the ignored LICENSE file is unavailable.
    let license = build_source.join("LICENSE");
    if !license.is_file() {
        fs::copy(source.join("README"), &license)?;
    }
    fs::create_dir_all(&zoneinfo)?;
    run_cmd(&build_source, "make", &["zic"])?;
    let zic = build_source.join("zic");
    let destination = format!("-d{}", zoneinfo.display());
    run_cmd(
        &build_source,
        path_str(&zic)?,
        &[
            destination.as_str(),
            "africa",
            "antarctica",
            "asia",
            "australasia",
            "backward",
            "etcetera",
            "europe",
            "northamerica",
            "southamerica",
        ],
    )?;
    for file in ["zone.tab", "zone1970.tab", "iso3166.tab"] {
        copy_preserving(&source.join(file), &zoneinfo.join(file))?;
    }
    if !zoneinfo.join("Etc/UTC").is_file() || !zoneinfo.join("America/Los_Angeles").is_file() {
        bail!("pinned tzdata build did not produce canonical zoneinfo files")
    }
    copy_tree_preserving(&zoneinfo, &staging.join("usr/share/zoneinfo"))?;
    copy_preserving(&license, &staging.join("usr/share/doc/tzdata/copyright"))?;
    Ok(())
}

/// Stage the complete WHENCE-described firmware closure. Firmware is the
/// documented source-closure exception: the authoritative, pinned upstream
/// tree and its redistribution metadata are retained even though most payload
/// files are device bytecode rather than preferred-form source.
pub(super) fn stage_linux_firmware(repo_root: &Path, staging: &Path) -> Result<()> {
    let source = repo_root.join("src/system/data/linux-firmware");
    let firmware = staging.join("usr/lib/firmware");
    if !source.join("WHENCE").is_file() || !source.join("copy-firmware.sh").is_file() {
        bail!("pinned linux-firmware source is missing WHENCE or its installer")
    }
    fs::create_dir_all(&firmware)?;
    run_cmd(
        &source,
        "sh",
        &["./copy-firmware.sh", "--zstd", path_str(&firmware)?],
    )?;
    if !firmware.join("intel").is_dir()
        || !firmware.join("amdgpu").is_dir()
        || !firmware
            .join("intel/iwlwifi/iwlwifi-so-a0-gf-a0-83.ucode.zst")
            .is_file()
    {
        bail!("linux-firmware staging lacks broad Intel/AMD firmware coverage")
    }
    let documentation = staging.join("usr/share/doc/linux-firmware");
    for name in ["WHENCE", "README.md"] {
        copy_preserving(&source.join(name), &documentation.join(name))?;
    }
    // LICENSE is globally ignored by the MattOS source tree. Preserve the
    // upstream licensing notice at the package boundary using README.md when
    // the ignored aggregate file is unavailable; individual LICENSES files
    // below remain authoritative for firmware-specific terms.
    let license = source.join("LICENSE");
    if license.is_file() {
        copy_preserving(&license, &documentation.join("LICENSE"))?;
    } else {
        copy_preserving(&source.join("README.md"), &documentation.join("LICENSE"))?;
    }
    copy_tree_preserving(&source.join("LICENSES"), &documentation.join("LICENSES"))?;
    for entry in fs::read_dir(&source)? {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if entry.path().is_file() && (name.starts_with("LICENCE.") || name.starts_with("LICENSE."))
        {
            copy_preserving(&entry.path(), &documentation.join(name.as_ref()))?;
        }
    }
    Ok(())
}

/// Regenerate the canonical database in an output-owned directory and require
/// it to match the signed upstream artifact before pairing it with that
/// artifact's detached signature and public redistribution metadata.
pub(crate) fn stage_wireless_regdb(repo_root: &Path, staging: &Path) -> Result<()> {
    let source = repo_root.join("src/system/data/wireless-regdb");
    let output = repo_root.join("out/build/wireless-regdb");
    remove_path_if_exists(&output)?;
    fs::create_dir_all(&output)?;
    run_cmd(
        &source,
        "python3",
        &[
            "db2fw.py",
            path_str(&output.join("regulatory.db"))?,
            "db.txt",
        ],
    )?;
    let generated = fs::read(output.join("regulatory.db"))?;
    let signed_payload = fs::read(source.join("regulatory.db"))?;
    if generated != signed_payload {
        bail!("generated wireless regulatory database does not match the pinned signed artifact")
    }
    let firmware = staging.join("usr/lib/firmware");
    copy_preserving(
        &output.join("regulatory.db"),
        &firmware.join("regulatory.db"),
    )?;
    copy_preserving(
        &source.join("regulatory.db.p7s"),
        &firmware.join("regulatory.db.p7s"),
    )?;
    let documentation = staging.join("usr/share/doc/wireless-regdb");
    let license = source.join("LICENSE");
    if license.is_file() {
        copy_preserving(&license, &documentation.join("copyright"))?;
    } else {
        // LICENSE files are globally ignored by the MattOS source tree; the
        // pinned upstream README is the retained licensing notice.
        copy_preserving(&source.join("README"), &documentation.join("copyright"))?;
    }
    copy_preserving(&source.join("db.txt"), &documentation.join("db.txt"))?;
    copy_preserving(
        &source.join("wens.key.pub.pem"),
        &documentation.join("wens.key.pub.pem"),
    )?;
    Ok(())
}

pub(crate) fn stage_brush(repo_root: &Path, staging: &Path) -> Result<()> {
    let bin_dir = staging.join("usr/bin");
    stage_executable(
        &repo_root.join("out/build/brush/cargo-target/release/brush"),
        &bin_dir.join("brush"),
        0o755,
    )?;
    // /usr/bin/sh belongs to dash; brush keeps its bash entry point.
    #[cfg(unix)]
    std::os::unix::fs::symlink("brush", bin_dir.join("bash"))?;
    Ok(())
}

pub(super) fn stage_base_files(repo_root: &Path, staging: &Path) -> Result<()> {
    let skeleton = repo_root.join("src/rootfs/skeleton/etc");
    for name in ["os-release", "hostname", "profile", "shells"] {
        copy_preserving(&skeleton.join(name), &staging.join("etc").join(name))?;
    }
    let user_skeleton = skeleton.join("skel");
    if user_skeleton.is_dir() {
        // /etc/skel is the single packaged source for defaults inherited by
        // installed users and explicitly mirrored into the pre-created live
        // account. Do not leave its contents as unowned rootfs overlay data.
        copy_tree_preserving(&user_skeleton, &staging.join("etc/skel"))?;
    }
    copy_preserving(
        &repo_root.join("src/system/packages/config/base-files/environment"),
        &staging.join("etc/environment"),
    )?;
    let config = repo_root.join("src/system/packages/config/base-files");
    copy_preserving(&config.join("issue"), &staging.join("etc/issue"))?;
    let conffiles = [
        "/etc/hostname",
        "/etc/profile",
        "/etc/shells",
        "/etc/issue",
        "/etc/environment",
    ];
    fs::write(
        staging.join("DEBIAN/conffiles"),
        format!("{}\n", conffiles.join("\n")),
    )?;
    Ok(())
}

pub(super) fn stage_systemd_runtime(repo_root: &Path, staging: &Path) -> Result<()> {
    let install = component_install(repo_root, "systemd");
    copy_tree_filtered(&install, staging, &|relative, _| {
        let path = relative.to_string_lossy();
        !path.starts_with("usr/include/")
            && !path.starts_with("usr/lib/x86_64-linux-gnu/pkgconfig/")
            && !path.starts_with("usr/share/pkgconfig/")
            && !path.starts_with("usr/lib/udev/hwdb.d/")
            && path != UDEV_HWDB_UNIT_REL
            && path != UDEV_HWDB_WANTS_REL
            && path != "usr/lib/udev/hwdb.bin"
            // mattos-compat deliberately owns the nspawn executable that it
            // wraps.  Keep the base systemd package disjoint so installing
            // both packages never creates a duplicate path owner.
            && path != "usr/bin/systemd-nspawn"
            // MattOS's libpam-runtime package owns the effective systemd-user
            // stack in /etc.  Do not also ship systemd's optional vendor
            // fallback, whose module closure is intentionally broader.
            && path != "usr/lib/pam.d/systemd-user"
            && !matches!(
                path.as_ref(),
                "usr/lib/x86_64-linux-gnu/libsystemd.so"
                    | "usr/lib/x86_64-linux-gnu/libsystemd.so.0"
                    | "usr/lib/x86_64-linux-gnu/libsystemd.so.0.44.0"
                    | "usr/lib/x86_64-linux-gnu/libudev.so"
                    | "usr/lib/x86_64-linux-gnu/libudev.so.1"
                    | "usr/lib/x86_64-linux-gnu/libudev.so.1.7.14"
            )
    })?;
    copy_preserving(
        &repo_root.join("src/system/systemd/LICENSE.LGPL2.1"),
        &staging.join("usr/share/doc/systemd/copyright"),
    )
}

pub(super) fn stage_mattos_base_runtime(repo_root: &Path, staging: &Path) -> Result<()> {
    stage_executable(
        &repo_root.join("out/build/init/cargo-target/release/mattos-init"),
        &staging.join("usr/libexec/mattos/rescue-init"),
        0o755,
    )?;
    for relative in [
        "usr/libexec/mattos/brush-login",
        "usr/libexec/mattos/validate-shell-env",
    ] {
        copy_preserving(
            &repo_root.join("src/rootfs/skeleton").join(relative),
            &staging.join(relative),
        )?;
        set_mode(staging.join(relative), 0o755)?;
    }
    for unit in [
        "mattos.target",
        "mattos-shell.service",
        "mattos-smoke.service",
    ] {
        copy_preserving(
            &repo_root.join("src/system/units").join(unit),
            &staging.join("usr/lib/systemd/system").join(unit),
        )?;
    }
    for (source, destination) in [
        (
            "src/system/network/resolved.conf",
            "etc/systemd/resolved.conf.d/10-mattos.conf",
        ),
        (
            "src/system/network/timesyncd.conf",
            "etc/systemd/timesyncd.conf.d/10-mattos.conf",
        ),
        ("src/system/network/nsswitch.conf", "etc/nsswitch.conf"),
        ("src/system/network/hosts", "etc/hosts"),
        ("src/system/network/networks", "etc/networks"),
        (
            "src/system/network/99-mattos-network.conf",
            "etc/sysctl.d/99-mattos-network.conf",
        ),
    ] {
        copy_preserving(&repo_root.join(source), &staging.join(destination))?;
    }
    Ok(())
}

/// A uutils command package: release binaries from the component's cargo
/// target, `(alias, target)` symlinks beside them, and its license files as
/// the package copyright.
pub(super) fn stage_uutils_commands(
    repo_root: &Path,
    staging: &Path,
    component: &str,
    binaries: &[(&str, &str)],
    aliases: &[(&str, &str)],
    licenses: &[&str],
) -> Result<()> {
    let release = repo_root.join("out/build").join(component).join("cargo-target/release");
    let bin_dir = staging.join("usr/bin");
    for (source, destination) in binaries {
        stage_executable(&release.join(source), &bin_dir.join(destination), 0o755)?;
    }
    for (alias, target) in aliases {
        std::os::unix::fs::symlink(target, bin_dir.join(alias))?;
    }
    let doc = staging.join("usr/share/doc").join(component);
    fs::create_dir_all(&doc)?;
    let mut copyright = String::new();
    for license in licenses {
        copyright.push_str(&fs::read_to_string(
            repo_root.join("src/userland").join(component).join(license),
        )?);
    }
    fs::write(doc.join("copyright"), copyright)?;
    Ok(())
}

/// Earlier `apt` packages shipped a disabled Debian archive source as a
/// conffile.  dpkg keeps an obsolete conffile, so remove it here while it is
/// still the shipped, fully disabled scaffold; an administrator's enabled
/// copy is left alone.
pub(crate) const APT_POSTINST: &str = "#!/bin/sh\nset -e\nif [ \"$1\" = configure ]; then\n    debian=\"${DPKG_ROOT:-}/etc/apt/sources.list.d/debian-trixie.sources\"\n    if [ -f \"$debian\" ] && grep -q '^Enabled: no' \"$debian\" && ! grep -q '^Enabled: yes' \"$debian\"; then\n        rm -f \"$debian\"\n    fi\nfi\n";

pub(super) const PLASMA_PROFILE_POSTINST: &str = "#!/bin/sh\nset -e\n[ -n \"${DPKG_ROOT:-}\" ] && exit 0\nif command -v systemctl >/dev/null 2>&1; then\n    if [ \"$(readlink /etc/systemd/system/display-manager.service 2>/dev/null || true)\" = \"/usr/lib/systemd/system/plasma-greeter.service\" ]; then rm /etc/systemd/system/display-manager.service; fi\n    systemctl enable plasmalogin.service >/dev/null\n    systemctl set-default graphical.target >/dev/null\nfi\n";

pub(super) fn stage_profile_package(repo_root: &Path, staging: &Path, package: &str) -> Result<()> {
    let profile = package.strip_prefix("mattos-").unwrap_or(package);
    if matches!(profile, "base" | "cli" | "plasma") {
        copy_preserving(
            &repo_root
                .join("src/system/packages/profiles")
                .join(format!("{profile}.toml")),
            &staging
                .join("usr/share/mattos/install-profiles")
                .join(format!("{profile}.toml")),
        )?;
    }
    if package == "mattos-build-essential" {
        let doc = staging.join("usr/share/doc/mattos-build-essential");
        fs::create_dir_all(&doc)?;
        fs::write(
            doc.join("README"),
            "MattOS build essentials: GCC, G++, Make, Binutils, pkgconf and CMake with\n\
             the C library headers, dash, GNU sed and mawk.  Enough to build ordinary\n\
             Autotools and CMake projects; add the -dev packages a project needs.\n",
        )?;
    }
    if package == "mattos-toolchain" {
        let doc = staging.join("usr/share/doc/mattos-toolchain");
        fs::create_dir_all(&doc)?;
        fs::write(
            doc.join("README"),
            "MattOS native development toolchain: GCC, Binutils, Clang/LLVM and Rust,\n\
             with their development headers.  The installer adds it to every installed\n\
             system; the live image omits it and keeps the packages on the medium.\n",
        )?;
    }
    if package == "mattos-plasma" {
        // Installing the desktop meta-package onto an existing CLI system is
        // a supported profile transition.  Offline installer composition
        // sets DPKG_ROOT and performs target activation itself; a normal APT
        // transaction should make the newly installed graphical profile the
        // next boot's default and enable its display-session service.
        let postinst = staging.join("DEBIAN/postinst");
        fs::write(&postinst, PLASMA_PROFILE_POSTINST)?;
        set_mode(postinst, 0o755)?;
    }
    Ok(())
}

#[cfg(test)]
#[test]
fn plasma_profile_package_activates_graphical_target_only_on_real_systems() {
    let source = include_str!("../../../../../system/packages/profiles/plasma.toml");
    assert!(source.contains("meta_package = \"mattos-plasma\""));
    assert!(PLASMA_PROFILE_POSTINST.contains("DPKG_ROOT"));
    assert!(PLASMA_PROFILE_POSTINST.contains("systemctl enable plasmalogin.service"));
    assert!(PLASMA_PROFILE_POSTINST.contains("display-manager.service"));
    assert!(PLASMA_PROFILE_POSTINST.contains("systemctl set-default graphical.target"));
}

pub(crate) fn stage_ca_certificates(repo_root: &Path, staging: &Path) -> Result<()> {
    let source = repo_root.join("src/system/network");
    let bundle = source.join("ca-certificates.crt");
    let metadata = source.join("ca-bundle.toml");
    let parsed: toml::Value = toml::from_str(&fs::read_to_string(&metadata)?)?;
    let expected_sha = parsed
        .get("sha256")
        .and_then(toml::Value::as_str)
        .ok_or_else(|| anyhow!("CA bundle metadata lacks sha256"))?;
    if sha256_file(&bundle)? != expected_sha {
        bail!("pinned CA bundle checksum does not match ca-bundle.toml")
    }
    let expected_count = parsed
        .get("certificate_count")
        .and_then(toml::Value::as_integer)
        .ok_or_else(|| anyhow!("CA bundle metadata lacks certificate_count"))?;
    let actual_count = fs::read_to_string(&bundle)?
        .matches("-----BEGIN CERTIFICATE-----")
        .count() as i64;
    if actual_count != expected_count {
        bail!("pinned CA bundle contains {actual_count} certificates; expected {expected_count}")
    }
    copy_preserving(&bundle, &staging.join("etc/ssl/certs/ca-certificates.crt"))?;
    let openssl_default = staging.join("etc/ssl/cert.pem");
    if let Some(parent) = openssl_default.parent() {
        fs::create_dir_all(parent)?;
    }
    std::os::unix::fs::symlink("certs/ca-certificates.crt", &openssl_default)?;
    copy_preserving(
        &metadata,
        &staging.join("usr/share/doc/ca-certificates/ca-bundle.toml"),
    )?;
    fs::write(
        staging.join("usr/share/doc/ca-certificates/UPDATE.md"),
        "Update only by replacing the pinned curl CA Extract input, then update the source URL, Mozilla data timestamp, SHA-256, certificate count, and MPL-2.0 license metadata in ca-bundle.toml. Ordinary builds never download a mutable CA bundle.\n",
    )?;
    Ok(())
}

pub(super) fn stage_dpkg(repo_root: &Path, staging: &Path) -> Result<()> {
    let install = repo_root.join("out/build/dpkg/install");
    for rel in DPKG_RUNTIME_PATHS {
        stage_executable(&install.join(rel), &staging.join(rel), 0o755)?;
    }
    copy_tree_preserving(
        &install.join("usr/share/dpkg"),
        &staging.join("usr/share/dpkg"),
    )?;
    fs::create_dir_all(staging.join("etc/dpkg/dpkg.cfg.d"))?;
    fs::create_dir_all(staging.join("etc/alternatives"))?;
    fs::create_dir_all(staging.join("var/lib/dpkg/alternatives"))?;
    copy_preserving(
        &repo_root.join("src/system/packages/config/dpkg/dpkg.cfg"),
        &staging.join("etc/dpkg/dpkg.cfg"),
    )?;
    fs::write(staging.join("DEBIAN/conffiles"), "/etc/dpkg/dpkg.cfg\n")?;
    validate_no_mutable_package_state(staging)
}

pub(super) fn stage_libapt_pkg(repo_root: &Path, staging: &Path) -> Result<()> {
    let source = repo_root.join("out/build/apt/install/usr/lib/x86_64-linux-gnu");
    let destination = staging.join("usr/lib/x86_64-linux-gnu");
    for name in ["libapt-pkg.so.7.0.0", "libapt-pkg.so.7.0", "libapt-pkg.so"] {
        copy_path_preserving(&source.join(name), &destination.join(name))?;
    }
    Ok(())
}

pub(super) fn stage_apt(repo_root: &Path, staging: &Path) -> Result<()> {
    let install = repo_root.join("out/build/apt/install");
    for rel in APT_RUNTIME_PATHS {
        stage_executable(&install.join(rel), &staging.join(rel), 0o755)?;
    }
    for rel in ["usr/lib/apt/planners", "usr/lib/apt/solvers"] {
        copy_tree_preserving(&install.join(rel), &staging.join(rel))?;
    }
    let libdir = "usr/lib/x86_64-linux-gnu";
    for name in ["libapt-private.so.0.0.0", "libapt-private.so.0.0"] {
        copy_path_preserving(
            &install.join(libdir).join(name),
            &staging.join(libdir).join(name),
        )?;
    }
    let config = repo_root.join("src/system/packages/config/apt");
    copy_preserving(
        &config.join("00-mattos-local.sources"),
        &staging.join("etc/apt/sources.list.d/00-mattos-local.sources"),
    )?;
    copy_preserving(
        &config.join("01mattos"),
        &staging.join("etc/apt/apt.conf.d/01mattos"),
    )?;
    copy_preserving(
        &config.join("mattos-hosted.sources"),
        &staging.join("etc/apt/sources.list.d/mattos-hosted.sources"),
    )?;
    copy_preserving(
        &config.join("00mattos-priority"),
        &staging.join("etc/apt/preferences.d/00mattos-priority"),
    )?;
    let installed_config = config.join("installed");
    for name in [
        "01mattos",
        "00mattos-priority",
        "00-mattos-local.sources",
        "mattos-hosted.sources",
    ] {
        copy_preserving(
            &installed_config.join(name),
            &staging.join("usr/share/mattos/apt/installed").join(name),
        )?;
    }
    copy_preserving(
        &config.join("keys/mattos-archive-keyring.asc"),
        &staging.join("usr/share/keyrings/mattos-archive-keyring.asc"),
    )?;
    let resources = repo_root.join("src/system/packages/config/apt/units");
    for unit in [
        "mattos-apt-daily.service",
        "mattos-apt-daily.timer",
        "mattos-apt-bootstrap.service",
        "mattos-apt-bootstrap.timer",
    ] {
        copy_preserving(
            &resources.join(unit),
            &staging.join("usr/lib/systemd/system").join(unit),
        )?;
    }
    for rel in [
        "etc/apt/auth.conf.d",
        "etc/apt/preferences.d",
        "etc/apt/trusted.gpg.d",
        "var/lib/apt/lists/partial",
        "var/cache/apt/archives/partial",
        "var/log/apt",
    ] {
        fs::create_dir_all(staging.join(rel))?;
    }
    fs::write(
        staging.join("DEBIAN/conffiles"),
        format!("{}\n", APT_CONFFILES.join("\n")),
    )?;
    fs::write(staging.join("DEBIAN/postinst"), APT_POSTINST)?;
    set_mode(staging.join("DEBIAN/postinst"), 0o755)?;
    validate_no_mutable_package_state(staging)
}

/// Apply the deliberately offline/live APT policy after package installation.
/// The apt package itself carries the installed policy templates; this overlay
/// makes the ISO root deterministic without relying on network availability.
pub(crate) fn apply_live_apt_policy(repo_root: &Path, rootfs: &Path) -> Result<()> {
    let config = repo_root.join("src/system/packages/config/apt");
    copy_preserving(
        &config.join("01mattos"),
        &rootfs.join("etc/apt/apt.conf.d/01mattos"),
    )?;
    copy_preserving(
        &config.join("00-mattos-local.sources"),
        &rootfs.join("etc/apt/sources.list.d/00-mattos-local.sources"),
    )?;
    copy_preserving(
        &config.join("mattos-hosted.sources"),
        &rootfs.join("etc/apt/sources.list.d/mattos-hosted.sources"),
    )?;
    copy_preserving(
        &config.join("00mattos-priority"),
        &rootfs.join("etc/apt/preferences.d/00mattos-priority"),
    )?;
    validate_live_apt_policy(rootfs)
}

pub(crate) fn validate_live_apt_policy(rootfs: &Path) -> Result<()> {
    let local = fs::read_to_string(rootfs.join("etc/apt/sources.list.d/00-mattos-local.sources"))?;
    let hosted = fs::read_to_string(rootfs.join("etc/apt/sources.list.d/mattos-hosted.sources"))?;
    let preferences = fs::read_to_string(rootfs.join("etc/apt/preferences.d/00mattos-priority"))?;
    let keyrings = rootfs.join("usr/share/keyrings");
    if !local.contains("URIs: file:/usr/share/mattos/repository")
        || local.contains("Enabled: no")
        || !local.contains("Trusted: yes")
        || !hosted.contains("Enabled: yes")
        || !hosted.contains("Signed-By: /usr/share/keyrings/mattos-archive-keyring.asc")
        || !keyrings.join("mattos-archive-keyring.asc").is_file()
        || rootfs.join("etc/apt/sources.list.d/debian-trixie.sources").exists()
        || !rootfs.join("usr/bin/gpgv").is_file()
        || fs::symlink_metadata(
            rootfs.join("etc/systemd/system/timers.target.wants/mattos-apt-daily.timer"),
        )
        .is_ok()
        || !preferences.contains("Pin-Priority: 990")
    {
        bail!("live APT policy does not enable both local and hosted MattOS repositories")
    }
    Ok(())
}

pub(crate) fn stage_udev_hwdb(repo_root: &Path, staging: &Path) -> Result<()> {
    let systemd_install = component_install(repo_root, "systemd");

    // Meson converts the authoritative imported systemd hwdb inputs into the
    // exact vendor-source closure selected by this pinned systemd revision.
    // Stage that closure into the package-owned output tree, then compile the
    // binary database there. The imported systemd tree is never writable.
    copy_tree_preserving(
        &systemd_install.join(UDEV_HWDB_SOURCE_REL),
        &staging.join(UDEV_HWDB_SOURCE_REL),
    )?;
    for rel in [UDEV_HWDB_UNIT_REL, UDEV_HWDB_WANTS_REL] {
        copy_path_preserving(&systemd_install.join(rel), &staging.join(rel))?;
    }
    generate_udev_hwdb(repo_root, staging)?;
    copy_preserving(
        &repo_root.join("src/system/systemd/LICENSE.LGPL2.1"),
        &staging.join("usr/share/doc/udev/copyright"),
    )?;
    validate_udev_hwdb_payload(repo_root, staging)
}

pub(super) fn generate_udev_hwdb(repo_root: &Path, root: &Path) -> Result<()> {
    let glibc_install = component_install(repo_root, "glibc");
    let systemd_install = component_install(repo_root, "systemd");
    let loader = glibc_install.join("lib64/ld-linux-x86-64.so.2");
    let generator = systemd_install.join("usr/bin/systemd-hwdb");
    let library_path = std::env::join_paths([
        glibc_install.join("usr/lib/x86_64-linux-gnu"),
        systemd_install.join("usr/lib/x86_64-linux-gnu"),
        systemd_install.join("usr/lib/x86_64-linux-gnu/systemd"),
    ])?;
    let root_arg = format!("--root={}", root.display());
    let loader_text = path_str(&loader)?;
    let generator_text = path_str(&generator)?;
    let library_path_text = library_path
        .to_str()
        .ok_or_else(|| anyhow!("systemd-hwdb library path is not valid UTF-8"))?;
    run_cmd(
        repo_root,
        loader_text,
        &[
            "--library-path",
            library_path_text,
            generator_text,
            &root_arg,
            "--usr",
            "--strict",
            "update",
        ],
    )
}

pub(crate) fn validate_udev_hwdb_payload(repo_root: &Path, root: &Path) -> Result<()> {
    let database = root.join(UDEV_HWDB_BINARY_REL);
    let bytes = fs::read(&database)
        .with_context(|| format!("prebuilt udev hwdb is missing at {}", database.display()))?;
    if bytes.len() < 1_000_000 || !bytes.starts_with(b"KSLPHHRH") {
        bail!("prebuilt udev hwdb has an invalid header or implausible size")
    }
    if path_entry_exists(&root.join("etc/udev/hwdb.bin")) {
        bail!("mutable /etc/udev/hwdb.bin must not be baked into the udev package")
    }

    let systemd_install = component_install(repo_root, "systemd");
    let glibc_install = component_install(repo_root, "glibc");
    let loader = glibc_install.join("lib64/ld-linux-x86-64.so.2");
    let generator = systemd_install.join("usr/bin/systemd-hwdb");
    let library_path = std::env::join_paths([
        glibc_install.join("usr/lib/x86_64-linux-gnu"),
        systemd_install.join("usr/lib/x86_64-linux-gnu"),
        systemd_install.join("usr/lib/x86_64-linux-gnu/systemd"),
    ])?;
    let output = Command::new(loader)
        .args(["--library-path"])
        .arg(library_path)
        .arg(generator)
        .arg(format!("--root={}", root.display()))
        .args(["query", UDEV_HWDB_TEST_MODALIAS])
        .env("LC_ALL", "C")
        .env("LANG", "C")
        .env("TZ", "UTC")
        .env("SOURCE_DATE_EPOCH", SOURCE_DATE_EPOCH.to_string())
        .output()
        .context("failed to query the prebuilt udev hwdb")?;
    if !output.status.success() {
        bail!("source-built systemd-hwdb rejected the prebuilt database")
    }
    let stdout = String::from_utf8(output.stdout)?;
    for required in ["Intel Corporation", "82540EM Gigabit Ethernet Controller"] {
        if !stdout.contains(required) {
            bail!("prebuilt udev hwdb query is missing {required}")
        }
    }
    Ok(())
}

pub(super) fn stage_gnupg(repo_root: &Path, staging: &Path) -> Result<()> {
    let install = component_install(repo_root, "gpgv");
    // Keep gpgv's dedicated package ownership intact while publishing the
    // OpenPGP engine and helpers GPGME needs for verified Flatpak remotes.
    copy_tree_filtered(
        &install.join("usr/bin"),
        &staging.join("usr/bin"),
        &|relative, metadata| {
            metadata.is_dir()
                || relative
                    .file_name()
                    .and_then(OsStr::to_str)
                    .is_some_and(|name| name != "gpgv")
        },
    )?;
    for relative in ["usr/libexec", "usr/sbin", "usr/share/gnupg"] {
        copy_tree_preserving(&install.join(relative), &staging.join(relative))?;
    }
    copy_preserving(
        &repo_root.join("src/system/security/gnupg/COPYING"),
        &staging.join("usr/share/doc/gnupg/copyright"),
    )
}

pub(super) fn stage_grub_package(repo_root: &Path, staging: &Path) -> Result<()> {
    for directory in ["usr", "etc"] {
        copy_tree_preserving(
            &component_install(repo_root, "grub").join(directory),
            &staging.join(directory),
        )?;
    }
    // GRUB's installed modinfo.sh is used by grub-install/mkimage, but its
    // generated build flags otherwise retain the disposable sysroot and
    // component install prefixes.  Normalize this metadata in the package
    // view only; the cache-owned GRUB build output remains untouched.
    normalize_staged_grub_modinfo(
        repo_root,
        &staging.join("usr/lib/grub/x86_64-efi/modinfo.sh"),
    )?;
    // These belong respectively to a global index and hybrid media, not the
    // installed x86_64 UEFI package. Do not exempt BIOS objects from ELF audit.
    remove_path_if_exists(&staging.join("usr/share/info/dir"))?;
    remove_path_if_exists(&staging.join("usr/lib/grub/i386-pc"))?;
    copy_preserving(
        &repo_root.join("src/boot/grub/upstream/COPYING"),
        &staging.join("usr/share/doc/grub-efi-amd64/copyright"),
    )?;
    let policy = repo_root.join("src/boot/grub/config");
    stage_executable(
        &policy.join("update-grub"),
        &staging.join("usr/sbin/update-grub"),
        0o755,
    )?;
    let mut conffiles = Vec::new();
    for entry in fs::read_dir(staging.join("etc/grub.d"))? {
        let path = entry?.path();
        if path.is_file() && path.file_name().is_some_and(|name| name != "README") {
            conffiles.push(format!("/{}", path.strip_prefix(staging)?.display()));
        }
    }
    for hook in ["postinst.d", "postrm.d"] {
        let relative = format!("etc/kernel/{hook}/zz-update-grub");
        stage_executable(&policy.join("kernel-hook"), &staging.join(&relative), 0o755)?;
        conffiles.push(format!("/{relative}"));
    }
    // Preserve administrator edits (especially 40_custom) across upgrades.
    // /etc/default/grub and generated /boot/grub/grub.cfg are installer/user
    // state and intentionally are not shipped or overwritten by this package.
    conffiles.sort();
    fs::write(
        staging.join("DEBIAN/conffiles"),
        format!("{}\n", conffiles.join("\n")),
    )?;
    Ok(())
}

pub(super) fn normalize_staged_grub_modinfo(repo_root: &Path, path: &Path) -> Result<()> {
    if !path.is_file() {
        return Ok(());
    }
    let build_root = repo_root.join("out/build");
    let sysroot = repo_root.join("out/sysroot").to_string_lossy().into_owned();
    let repo_root_text = repo_root.to_string_lossy().into_owned();
    let mut contents = fs::read_to_string(path)?;
    contents = contents.replace(&sysroot, "/");
    if let Ok(entries) = fs::read_dir(&build_root) {
        for entry in entries {
            let install_prefix = entry?.path().join("install/usr");
            if install_prefix.is_dir() {
                let install_prefix_text = install_prefix.to_string_lossy().into_owned();
                contents = contents.replace(&install_prefix_text, "/usr");
            }
        }
    }
    contents = contents.replace(&repo_root_text, "/usr/src/mattos");
    if contents.contains(&repo_root_text) {
        bail!(
            "GRUB metadata {} retains the host build root",
            path.display()
        );
    }
    fs::write(path, contents)?;
    Ok(())
}

pub(super) fn stage_wpa_supplicant(repo_root: &Path, staging: &Path) -> Result<()> {
    copy_tree_preserving(
        &component_install(repo_root, "wpa-supplicant").join("usr"),
        &staging.join("usr"),
    )?;
    // Upstream COPYING refers to README for the current BSD license terms.
    copy_preserving(
        &repo_root.join("src/system/network/hostap/wpa_supplicant/README"),
        &staging.join("usr/share/doc/wpasupplicant/copyright"),
    )?;
    for relative in WPA_RUNTIME_FILES {
        if !staging.join(relative).is_file() {
            bail!("wpasupplicant package is missing /{relative}");
        }
    }
    Ok(())
}

pub(super) const WPA_RUNTIME_FILES: &[&str] = &[
    "usr/sbin/wpa_supplicant",
    "usr/sbin/wpa_cli",
    "usr/sbin/wpa_passphrase",
    "usr/share/dbus-1/system-services/fi.w1.wpa_supplicant1.service",
    "usr/share/dbus-1/system.d/wpa_supplicant.conf",
    "usr/lib/systemd/system/wpa_supplicant.service",
];

#[cfg(test)]
mod wifi_payload_tests {
    use super::*;
    #[test]
    fn grub_preserves_custom_configuration_and_keeps_bios_objects_out_of_efi_package() {
        let fixture = tempfile::tempdir().unwrap();
        let install = fixture.path().join("out/build/grub/install");
        for relative in [
            "usr/lib/grub/i386-pc/linux.mod",
            "usr/lib/grub/x86_64-efi/linux.mod",
            "usr/share/info/dir",
            "etc/grub.d/00_header",
            "etc/grub.d/40_custom",
            "etc/grub.d/README",
        ] {
            let path = install.join(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, relative).unwrap();
        }
        for relative in [
            "src/boot/grub/upstream/COPYING",
            "src/boot/grub/config/update-grub",
            "src/boot/grub/config/kernel-hook",
        ] {
            let path = fixture.path().join(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, "fixture").unwrap();
        }
        let staging = fixture.path().join("payload");
        fs::create_dir_all(staging.join("DEBIAN")).unwrap();
        stage_grub_package(fixture.path(), &staging).unwrap();
        assert!(!staging.join("usr/lib/grub/i386-pc").exists());
        assert!(!staging.join("usr/share/info/dir").exists());
        assert!(staging.join("usr/lib/grub/x86_64-efi/linux.mod").is_file());
        let files = fs::read_to_string(staging.join("DEBIAN/conffiles")).unwrap();
        assert_eq!(
            files,
            "/etc/grub.d/00_header\n/etc/grub.d/40_custom\n/etc/kernel/postinst.d/zz-update-grub\n/etc/kernel/postrm.d/zz-update-grub\n"
        );
        assert!(!staging.join("etc/default/grub").exists());
        assert!(!staging.join("boot/grub/grub.cfg").exists());
        assert!(
            staging
                .join("usr/share/doc/grub-efi-amd64/copyright")
                .is_file()
        );
    }
    #[test]
    fn stages_the_complete_directory_tree_and_rejects_missing_activation_files() {
        let fixture = tempfile::tempdir().unwrap();
        let install = fixture.path().join("out/build/wpa-supplicant/install");
        let license = fixture
            .path()
            .join("src/system/network/hostap/wpa_supplicant/README");
        fs::create_dir_all(license.parent().unwrap()).unwrap();
        fs::write(&license, "upstream license").unwrap();
        for relative in WPA_RUNTIME_FILES {
            let path = install.join(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, relative).unwrap();
        }
        let staging = fixture.path().join("payload");
        stage_wpa_supplicant(fixture.path(), &staging).unwrap();
        assert_eq!(
            fs::read_to_string(staging.join("usr/share/doc/wpasupplicant/copyright")).unwrap(),
            "upstream license"
        );
        for relative in WPA_RUNTIME_FILES {
            assert_eq!(
                fs::read_to_string(staging.join(relative)).unwrap(),
                *relative
            );
        }
        fs::remove_file(install.join(WPA_RUNTIME_FILES[3])).unwrap();
        let fresh = fixture.path().join("missing-activation");
        assert!(stage_wpa_supplicant(fixture.path(), &fresh).is_err());
    }
}

pub(super) fn stage_network_manager(repo_root: &Path, staging: &Path) -> Result<()> {
    let install = component_install(repo_root, "networkmanager");
    copy_tree_preserving(&install.join("usr"), &staging.join("usr"))?;
    copy_tree_preserving(&install.join("etc"), &staging.join("etc"))?;
    for rel in [
        "usr/sbin/NetworkManager",
        "usr/bin/nmcli",
        "usr/lib/systemd/system/NetworkManager.service",
        "usr/lib/systemd/system/NetworkManager-wait-online.service",
    ] {
        if !staging.join(rel).exists() {
            bail!("network-manager package is missing /{rel}");
        }
    }
    Ok(())
}

pub(super) fn stage_terminfo(repo_root: &Path, staging: &Path) -> Result<()> {
    let source = component_install(repo_root, "ncurses").join("usr/share/terminfo");
    for terminal in TERMINFO_ENTRIES {
        let first = terminal.as_bytes()[0];
        let candidates = [
            source.join(char::from(first).to_string()).join(terminal),
            source.join(format!("{first:x}")).join(terminal),
        ];
        let entry = candidates
            .iter()
            .find(|candidate| candidate.is_file())
            .ok_or_else(|| {
                anyhow!(
                    "terminfo entry {terminal} missing from {}",
                    source.display()
                )
            })?;
        let relative = entry.strip_prefix(&source)?;
        copy_preserving(entry, &staging.join("usr/share/terminfo").join(relative))?;
    }
    Ok(())
}

pub(super) fn stage_procps(repo_root: &Path, staging: &Path) -> Result<()> {
    stage_runtime_paths(repo_root, staging, "procps-ng", PROCPS_RUNTIME_PATHS)?;
    copy_preserving(
        &repo_root.join("src/userland/procps-ng/sysctl.conf"),
        &staging.join("etc/sysctl.conf"),
    )?;
    fs::write(staging.join("DEBIAN/conffiles"), "/etc/sysctl.conf\n")?;
    Ok(())
}

pub(super) fn stage_dbus_broker(repo_root: &Path, staging: &Path) -> Result<()> {
    let install = component_install(repo_root, "dbus-broker");
    for rel in ["usr/bin/dbus-broker", "usr/bin/dbus-broker-launch"] {
        copy_path_preserving(&install.join(rel), &staging.join(rel))?;
    }
    let dbus = repo_root.join("src/system/dbus");
    copy_preserving(
        &dbus.join("config/system.conf"),
        &staging.join("etc/dbus-1/system.conf"),
    )?;
    copy_preserving(
        &dbus.join("config/dbus.conf"),
        &staging.join("usr/lib/sysusers.d/dbus.conf"),
    )?;
    fs::create_dir_all(staging.join("usr/lib/tmpfiles.d"))?;
    fs::write(
        staging.join("usr/lib/tmpfiles.d/dbus.conf"),
        "d /run/dbus 0755 root root -\n",
    )?;
    copy_tree_preserving(&dbus.join("units"), &staging.join("usr/lib/systemd/system"))?;
    let session = repo_root.join("src/system/session");
    copy_preserving(
        &session.join("dbus/session.conf"),
        &staging.join("usr/share/dbus-1/session.conf"),
    )?;
    copy_tree_preserving(
        &session.join("user-units"),
        &staging.join("usr/lib/systemd/user"),
    )?;
    stage_dbus_service_links(staging)?;
    for rel in [
        "etc/dbus-1/system.d",
        "etc/dbus-1/session.d",
        "usr/share/dbus-1/system-services",
        "usr/share/dbus-1/system.d",
        "usr/share/dbus-1/session.d",
        "usr/share/dbus-1/services",
    ] {
        fs::create_dir_all(staging.join(rel))?;
    }
    fs::write(
        staging.join("DEBIAN/conffiles"),
        "/etc/dbus-1/system.conf\n",
    )?;
    Ok(())
}

#[cfg(unix)]
pub(super) fn stage_dbus_service_links(staging: &Path) -> Result<()> {
    use std::os::unix::fs::symlink;

    // These aliases and enablement links are part of the D-Bus provider, not
    // an image-overlay concern. Package-composed targets never run the
    // live-root overlay helper that previously supplied them.
    symlink(
        "dbus-broker.service",
        staging.join("usr/lib/systemd/system/dbus.service"),
    )?;
    symlink(
        "dbus-broker.service",
        staging.join("usr/lib/systemd/user/dbus.service"),
    )?;
    let system_wants = staging.join("usr/lib/systemd/system/sockets.target.wants");
    fs::create_dir_all(&system_wants)?;
    symlink("../dbus.socket", system_wants.join("dbus.socket"))?;
    let user_wants = staging.join("usr/lib/systemd/user/sockets.target.wants");
    fs::create_dir_all(&user_wants)?;
    symlink("../dbus.socket", user_wants.join("dbus.socket"))?;
    Ok(())
}

#[cfg(not(unix))]
pub(super) fn stage_dbus_service_links(_staging: &Path) -> Result<()> {
    bail!("D-Bus package service links require a Unix build host")
}

#[cfg(all(test, unix))]
#[test]
pub(super) fn dbus_package_owns_provider_aliases_and_socket_enablement() {
    let staging = tempfile::tempdir().expect("staging");
    fs::create_dir_all(staging.path().join("usr/lib/systemd/system"))
        .expect("system unit directory");
    fs::create_dir_all(staging.path().join("usr/lib/systemd/user")).expect("user unit directory");
    stage_dbus_service_links(staging.path()).expect("D-Bus service links");

    for (path, target) in [
        ("usr/lib/systemd/system/dbus.service", "dbus-broker.service"),
        ("usr/lib/systemd/user/dbus.service", "dbus-broker.service"),
        (
            "usr/lib/systemd/system/sockets.target.wants/dbus.socket",
            "../dbus.socket",
        ),
        (
            "usr/lib/systemd/user/sockets.target.wants/dbus.socket",
            "../dbus.socket",
        ),
    ] {
        assert_eq!(
            fs::read_link(staging.path().join(path)).expect("service link"),
            Path::new(target)
        );
    }
}

pub(super) fn stage_pam_modules(repo_root: &Path, staging: &Path) -> Result<()> {
    let source =
        component_install(repo_root, "linux-pam").join("usr/lib/x86_64-linux-gnu/security");
    let destination = staging.join("usr/lib/x86_64-linux-gnu/security");
    for module in PAM_MODULES {
        copy_preserving(&source.join(module), &destination.join(module))?;
    }
    Ok(())
}

#[test]
pub(super) fn pam_module_package_contains_modules_referenced_by_plasma_login_policy() {
    let plasma_login_pam =
        include_str!("../../../../../system/session/plasma-login-manager/plasmalogin.pam");
    for module in [
        "pam_unix.so",
        "pam_limits.so",
        "pam_env.so",
        "pam_nologin.so",
    ] {
        assert!(
            PAM_MODULES.contains(&module),
            "PAM package must stage {module}, referenced by Plasma Login Manager policy"
        );
    }
    assert!(plasma_login_pam.contains("session    required     pam_limits.so"));
}

pub(super) fn stage_pam_runtime(repo_root: &Path, staging: &Path) -> Result<()> {
    copy_path_preserving(
        &component_install(repo_root, "linux-pam").join("usr/sbin/unix_chkpwd"),
        &staging.join("usr/sbin/unix_chkpwd"),
    )?;
    let pam_policy = repo_root.join("src/system/auth/config/pam.d");
    copy_tree_preserving(&pam_policy, &staging.join("etc/pam.d"))?;
    // Linux-PAM is built with its upstream vendor configuration directory at
    // /usr/share/pam.  Ship the source-built pam_env defaults there so PAM
    // does not warn on every login while still allowing /etc/security to
    // override them locally.
    copy_preserving(
        &component_install(repo_root, "linux-pam").join("usr/share/pam/security/pam_env.conf"),
        &staging.join("usr/share/pam/security/pam_env.conf"),
    )?;
    let mut conffiles = fs::read_dir(&pam_policy)?
        .map(|entry| {
            Ok(format!(
                "/etc/pam.d/{}",
                entry?.file_name().to_string_lossy()
            ))
        })
        .collect::<Result<Vec<_>>>()?;
    conffiles.sort();
    fs::write(
        staging.join("DEBIAN/conffiles"),
        format!("{}\n", conffiles.join("\n")),
    )?;
    Ok(())
}

pub(super) fn stage_shadow(repo_root: &Path, staging: &Path) -> Result<()> {
    stage_runtime_paths(repo_root, staging, "shadow", SHADOW_RUNTIME_PATHS)?;
    let config = repo_root.join("src/system/auth/config");
    copy_preserving(&config.join("login.defs"), &staging.join("etc/login.defs"))?;
    copy_preserving(
        &config.join("default/useradd"),
        &staging.join("etc/default/useradd"),
    )?;
    fs::write(
        staging.join("DEBIAN/conffiles"),
        "/etc/login.defs\n/etc/default/useradd\n",
    )?;
    for rel in ["usr/bin/passwd", "usr/bin/newgrp"] {
        set_mode(staging.join(rel), 0o4755)?;
    }
    validate_no_mutable_system_state(staging)
}

/// Subordinate IDs for rootless containers: when installed on a running
/// system, create /etc/subuid and /etc/subgid (useradd then allocates a range
/// for every new user) and give each existing regular user a range.
pub(super) const UIDMAP_POSTINST: &str = r#"#!/bin/sh
set -e
[ -n "${DPKG_ROOT:-}" ] && exit 0
for file in /etc/subuid /etc/subgid; do
    [ -e "$file" ] || { : > "$file"; chmod 0644 "$file"; }
done
getent passwd | while IFS=: read -r user _ uid _; do
    [ "$uid" -ge 1000 ] && [ "$uid" -lt 60000 ] || continue
    start=$((100000 + (uid - 1000) * 65536))
    end=$((start + 65535))
    grep -q "^$user:" /etc/subuid || usermod --add-subuids "$start-$end" "$user"
    grep -q "^$user:" /etc/subgid || usermod --add-subgids "$start-$end" "$user"
done
exit 0
"#;

/// Debian's uidmap: shadow's setuid newuidmap and newgidmap, which rootless
/// Podman and other user-namespace tools use to map subordinate IDs.
pub(super) fn stage_uidmap(repo_root: &Path, staging: &Path) -> Result<()> {
    let install = component_install(repo_root, "shadow").join("usr");
    for program in ["newuidmap", "newgidmap"] {
        let destination = staging.join("usr/bin").join(program);
        stage_executable(&install.join("bin").join(program), &destination, 0o755)?;
        set_mode(destination, 0o4755)?;
    }
    let postinst = staging.join("DEBIAN/postinst");
    fs::create_dir_all(postinst.parent().expect("postinst has a parent"))?;
    fs::write(&postinst, UIDMAP_POSTINST)?;
    set_mode(postinst, 0o755)?;
    copy_preserving(
        &repo_root.join("src/system/auth/shadow/COPYING"),
        &staging.join("usr/share/doc/uidmap/copyright"),
    )
}

pub(super) fn stage_sudo_rs(repo_root: &Path, staging: &Path) -> Result<()> {
    stage_runtime_paths(
        repo_root,
        staging,
        "sudo-rs",
        &["usr/bin/sudo", "usr/bin/visudo"],
    )?;
    let config = repo_root.join("src/system/auth/config");
    copy_preserving(&config.join("sudoers"), &staging.join("etc/sudoers"))?;
    copy_preserving(
        &config.join("sudoers.d/README"),
        &staging.join("etc/sudoers.d/README"),
    )?;
    fs::write(
        staging.join("DEBIAN/conffiles"),
        "/etc/sudoers\n/etc/sudoers.d/README\n",
    )?;
    set_mode(staging.join("usr/bin/sudo"), 0o4755)?;
    set_mode(staging.join("etc/sudoers"), 0o440)?;
    set_mode(staging.join("etc/sudoers.d"), 0o750)?;
    set_mode(staging.join("etc/sudoers.d/README"), 0o440)?;
    Ok(())
}

pub(super) fn stage_util_linux_auth(repo_root: &Path, staging: &Path) -> Result<()> {
    stage_runtime_paths(repo_root, staging, "util-linux", UTIL_LINUX_AUTH_PATHS)?;
    for rel in ["usr/bin/login", "usr/bin/su"] {
        set_mode(staging.join(rel), 0o4755)?;
    }
    Ok(())
}

pub(super) fn stage_openssh_server(repo_root: &Path, staging: &Path) -> Result<()> {
    stage_runtime_paths(repo_root, staging, "openssh", OPENSSH_SERVER_RUNTIME_PATHS)?;
    let config = repo_root.join("src/system/network/openssh");
    copy_preserving(
        &config.join("sshd_config"),
        &staging.join("etc/ssh/sshd_config"),
    )?;
    copy_preserving(&config.join("ssh-pam"), &staging.join("etc/pam.d/sshd"))?;
    copy_preserving(
        &config.join("ssh.service"),
        &staging.join("usr/lib/systemd/system/ssh.service"),
    )?;
    copy_preserving(
        &config.join("openssh-sysusers.conf"),
        &staging.join("usr/lib/sysusers.d/openssh.conf"),
    )?;
    fs::create_dir_all(staging.join("etc/ssh/sshd_config.d"))?;
    fs::write(
        staging.join("DEBIAN/conffiles"),
        "/etc/ssh/sshd_config\n/etc/pam.d/sshd\n",
    )?;
    Ok(())
}

pub(super) fn stage_iproute2(repo_root: &Path, staging: &Path) -> Result<()> {
    stage_runtime_paths(repo_root, staging, "iproute2", IPROUTE2_RUNTIME_PATHS)?;
    copy_tree_preserving(
        &component_install(repo_root, "iproute2").join("usr/share/iproute2"),
        &staging.join("usr/share/iproute2"),
    )
}

pub(super) fn stage_linux_modules(repo_root: &Path, staging: &Path) -> Result<()> {
    let release = fs::read_to_string(repo_root.join("out/build/linux/kernel-release"))?;
    let release = release.trim();
    if release != MATTOS_KERNEL_RELEASE {
        bail!("kernel module package name does not match built release {release}");
    }
    let source = repo_root
        .join("out/build/linux/modules/usr/lib/modules")
        .join(release);
    copy_tree_preserving(&source, &staging.join("usr/lib/modules").join(release))?;
    copy_preserving(
        &repo_root.join("src/kernel/linux/COPYING"),
        &staging.join(format!("usr/share/doc/linux-modules-{release}/copyright")),
    )
}

pub(crate) fn stage_coreutils(repo_root: &Path, staging: &Path) -> Result<()> {
    let source = resolve_coreutils_multicall(repo_root)?;
    stage_executable(&source, &staging.join("usr/bin/coreutils"), 0o755)?;
    let applets = package_coreutils_applets(&source)?;
    #[cfg(unix)]
    for applet in applets {
        let path = staging.join("usr/bin").join(&applet);
        if path_entry_exists(&path) {
            bail!("duplicate coreutils command alias {applet}")
        }
        std::os::unix::fs::symlink("coreutils", path)?;
    }
    Ok(())
}

#[cfg(test)]
mod license_notice_tests {
    use super::*;
    use crate::performance::command_recorder::{self, RecordedCommand};

    fn write(path: &Path, body: &[u8]) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, body).unwrap();
    }

    fn tzdata_fixture(root: &Path, with_license: bool) {
        let source = root.join("src/system/data/tzdata");
        for name in [
            "README",
            "zone.tab",
            "zone1970.tab",
            "iso3166.tab",
            "africa",
        ] {
            write(&source.join(name), name.as_bytes());
        }
        if with_license {
            write(&source.join("LICENSE"), b"upstream LICENSE");
        }
    }

    /// The source mirror is a real rsync; zic writes the zones named on its
    /// command line below `-d<dir>`.
    fn zic(command: &RecordedCommand) -> Result<()> {
        if command.program == "rsync" {
            return command.run_for_real();
        }
        if command.program.ends_with("/zic") {
            let directory = command.args[0].strip_prefix("-d").unwrap();
            for zone in ["Etc/UTC", "America/Los_Angeles"] {
                write(&Path::new(directory).join(zone), b"TZif");
            }
        }
        Ok(())
    }

    #[test]
    fn tzdata_ships_its_readme_as_the_notice_when_the_license_is_not_imported() {
        for (with_license, notice) in [
            (false, "README".as_bytes()),
            (true, b"upstream LICENSE".as_slice()),
        ] {
            let root = tempfile::tempdir().unwrap();
            tzdata_fixture(root.path(), with_license);
            let staging = root.path().join("payload");
            let (result, commands) =
                command_recorder::record(zic, || stage_tzdata(root.path(), &staging));
            result.unwrap();
            assert_eq!(
                fs::read(staging.join("usr/share/doc/tzdata/copyright")).unwrap(),
                notice
            );
            assert!(staging.join("usr/share/zoneinfo/Etc/UTC").is_file());
            assert!(
                commands
                    .iter()
                    .any(|command| command.program == "make" && command.has_arg("zic"))
            );
            let compile = commands
                .iter()
                .find(|command| command.program.ends_with("/zic"))
                .unwrap();
            assert!(compile.has_arg("africa") && compile.has_arg("backward"));
            assert!(
                !root.path().join("src/system/data/tzdata/LICENSE").exists() || with_license,
                "the imported source tree is never modified"
            );
        }
    }

    fn firmware_fixture(root: &Path, with_license: bool) {
        let source = root.join("src/system/data/linux-firmware");
        for name in [
            "WHENCE",
            "copy-firmware.sh",
            "README.md",
            "LICENSES/GPL-2.0",
            "LICENCE.iwlwifi",
        ] {
            write(&source.join(name), name.as_bytes());
        }
        if with_license {
            write(&source.join("LICENSE"), b"aggregate LICENSE");
        }
    }

    /// copy-firmware.sh installs the firmware tree into its last argument.
    fn copy_firmware(command: &RecordedCommand) -> Result<()> {
        if command.has_arg("./copy-firmware.sh") {
            let firmware = Path::new(command.args.last().unwrap());
            write(
                &firmware.join("intel/iwlwifi/iwlwifi-so-a0-gf-a0-83.ucode.zst"),
                b"fw",
            );
            fs::create_dir_all(firmware.join("amdgpu"))?;
        }
        Ok(())
    }

    #[test]
    fn linux_firmware_keeps_every_license_and_falls_back_to_its_readme() {
        for (with_license, notice) in [
            (false, "README.md".as_bytes()),
            (true, b"aggregate LICENSE".as_slice()),
        ] {
            let root = tempfile::tempdir().unwrap();
            firmware_fixture(root.path(), with_license);
            let staging = root.path().join("payload");
            let (result, commands) = command_recorder::record(copy_firmware, || {
                stage_linux_firmware(root.path(), &staging)
            });
            result.unwrap();
            let documentation = staging.join("usr/share/doc/linux-firmware");
            assert_eq!(fs::read(documentation.join("LICENSE")).unwrap(), notice);
            assert!(documentation.join("LICENSES/GPL-2.0").is_file());
            assert!(documentation.join("LICENCE.iwlwifi").is_file());
            assert!(commands[0].has_arg("--zstd"));
        }
    }

    fn regdb_fixture(root: &Path, with_license: bool) -> PathBuf {
        let source = root.join("src/system/data/wireless-regdb");
        for name in [
            "db2fw.py",
            "db.txt",
            "regulatory.db.p7s",
            "README",
            "wens.key.pub.pem",
        ] {
            write(&source.join(name), name.as_bytes());
        }
        write(&source.join("regulatory.db"), b"signed database");
        if with_license {
            write(&source.join("LICENSE"), b"regdb LICENSE");
        }
        source
    }

    /// db2fw.py regenerates the database at its first argument.
    fn db2fw(contents: &'static [u8]) -> impl FnMut(&RecordedCommand) -> Result<()> {
        move |command| {
            if command.has_arg("db2fw.py") {
                write(Path::new(&command.args[1]), contents);
            }
            Ok(())
        }
    }

    #[test]
    fn wireless_regdb_ships_a_notice_and_only_a_database_matching_the_signed_one() {
        for (with_license, notice) in [
            (false, "README".as_bytes()),
            (true, b"regdb LICENSE".as_slice()),
        ] {
            let root = tempfile::tempdir().unwrap();
            regdb_fixture(root.path(), with_license);
            let staging = root.path().join("payload");
            let (result, _) = command_recorder::record(db2fw(b"signed database"), || {
                stage_wireless_regdb(root.path(), &staging)
            });
            result.unwrap();
            assert_eq!(
                fs::read(staging.join("usr/share/doc/wireless-regdb/copyright")).unwrap(),
                notice
            );
            assert!(staging.join("usr/lib/firmware/regulatory.db.p7s").is_file());
        }
        let root = tempfile::tempdir().unwrap();
        regdb_fixture(root.path(), false);
        let (result, _) = command_recorder::record(db2fw(b"tampered database"), || {
            stage_wireless_regdb(root.path(), &root.path().join("payload"))
        });
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("does not match the pinned signed artifact")
        );
    }
}

#[cfg(test)]
mod udev_hwdb_tests {
    use super::*;
    use crate::performance::command_recorder;

    #[test]
    fn the_hwdb_is_compiled_by_the_mattos_loader_into_the_package_root_only() {
        let root = tempfile::tempdir().unwrap();
        let payload = root.path().join("payload");
        let (result, commands) =
            command_recorder::record(|_| Ok(()), || generate_udev_hwdb(root.path(), &payload));
        result.unwrap();
        let [command] = commands.as_slice() else {
            panic!("one hwdb compile, got {commands:?}")
        };
        assert!(
            command
                .program
                .ends_with("out/build/glibc/install/lib64/ld-linux-x86-64.so.2")
        );
        assert!(command.args[1].contains("out/build/glibc/install/usr/lib/x86_64-linux-gnu"));
        assert!(command.args[2].ends_with("out/build/systemd/install/usr/bin/systemd-hwdb"));
        for flag in [
            format!("--root={}", payload.display()).as_str(),
            "--usr",
            "--strict",
            "update",
        ] {
            assert!(command.has_arg(flag), "systemd-hwdb must receive {flag}");
        }
    }

    #[test]
    fn a_small_or_foreign_or_mutable_hwdb_is_rejected_before_it_is_queried() {
        let root = tempfile::tempdir().unwrap();
        let database = root.path().join(UDEV_HWDB_BINARY_REL);
        fs::create_dir_all(database.parent().unwrap()).unwrap();
        fs::write(&database, b"KSLPHHRH tiny").unwrap();
        assert!(validate_udev_hwdb_payload(root.path(), root.path()).is_err());
        let mut large = b"NOTAHWDB".to_vec();
        large.resize(1_000_001, 0);
        fs::write(&database, &large).unwrap();
        assert!(validate_udev_hwdb_payload(root.path(), root.path()).is_err());
        large[..8].copy_from_slice(b"KSLPHHRH");
        fs::write(&database, &large).unwrap();
        fs::create_dir_all(root.path().join("etc/udev")).unwrap();
        fs::write(root.path().join("etc/udev/hwdb.bin"), b"mutable").unwrap();
        let error = validate_udev_hwdb_payload(root.path(), root.path())
            .unwrap_err()
            .to_string();
        assert!(error.contains("mutable /etc/udev/hwdb.bin"), "{error}");
    }
}

/// A base command package: its binary, command aliases (the `sh` and `awk`
/// names Debian selects with diversions/alternatives are plain symlinks
/// here), manual page and license notice.
pub(super) fn stage_base_command(
    repo_root: &Path,
    staging: &Path,
    package: &str,
    binary: &str,
    aliases: &[&str],
    license: &str,
) -> Result<()> {
    let install = component_install(repo_root, package).join("usr");
    let bin_dir = staging.join("usr/bin");
    stage_executable(&install.join("bin").join(binary), &bin_dir.join(binary), 0o755)?;
    for alias in aliases {
        std::os::unix::fs::symlink(binary, bin_dir.join(alias))?;
    }
    let manual = install.join("share/man/man1").join(format!("{binary}.1"));
    if manual.is_file() {
        copy_preserving(&manual, &staging.join("usr/share/man/man1").join(format!("{binary}.1")))?;
    }
    copy_preserving(
        &repo_root.join(license),
        &staging.join("usr/share/doc").join(package).join("copyright"),
    )
}
