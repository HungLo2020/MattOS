//! Package versions for components imported from a moving branch.
//!
//! A snapshot of `main` has no release tag, so its Debian upstream version is
//! built from what the tree declares and when the pinned commit was made:
//! `<declared>+git<YYYYMMDD>.<HHMMSS>.<commit>`.  The declared version orders
//! snapshots across upstream release cycles and the committer time orders them
//! within one; the commit only makes the version name its source.

use anyhow::{Context, Result, anyhow, bail};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

/// Where a component's tree declares its upstream version.
#[derive(Clone, Copy, Debug)]
enum Declaration {
    /// meson.build, or else configure.ac.
    BuildSystem,
    /// Kbuild's `VERSION`, `PATCHLEVEL`, `SUBLEVEL` and `EXTRAVERSION`.
    KernelMakefile,
    /// `NAME=value` lines of a makefile, joined with `.`.
    MakeVariables(&'static str, &'static [&'static str]),
    /// `#define NAME value` lines of a C header, joined with `.`.
    CDefines(&'static str, &'static [&'static str]),
    /// The first string literal in a C file.
    CString(&'static str),
    /// `[package] version` of a Cargo manifest.
    Cargo(&'static str),
    /// ncurses `VERSION`: ABI, release and patch date, tab separated.
    NcursesVersionFile,
    /// The newest entry of `debian/changelog`.
    DebianChangelog,
    /// The newest `NEWS` heading with this prefix that names a version.
    NewsHeading(&'static str),
}

/// Components whose tree does not declare its version through meson.build or
/// configure.ac (or declares it there only through `git describe`).
fn declaration(component: &str) -> Declaration {
    match component {
        "linux" | "linux-uapi" => Declaration::KernelMakefile,
        "gmp" => Declaration::CDefines(
            "gmp-h.in",
            &["__GNU_MP_VERSION", "__GNU_MP_VERSION_MINOR", "__GNU_MP_VERSION_PATCHLEVEL"],
        ),
        "libcap" => Declaration::MakeVariables("Make.Rules", &["VERSION", "MINOR"]),
        "iproute2" => Declaration::CString("include/version.h"),
        "ncurses" => Declaration::NcursesVersionFile,
        "pop-fonts" => Declaration::DebianChangelog,
        "procps-ng" => Declaration::NewsHeading("procps-ng-"),
        "util-linux" => Declaration::NewsHeading("util-linux "),
        "greetd" => Declaration::Cargo("greetd/Cargo.toml"),
        _ => Declaration::BuildSystem,
    }
}

/// The Debian upstream version of a snapshot of `component`, imported into
/// `tree` at `commit`, committed at `committed_at_utc` (RFC 3339, UTC).
pub(super) fn snapshot_upstream_version(
    component: &str,
    tree: &Path,
    commit: &str,
    committed_at_utc: Option<&str>,
) -> Result<String> {
    let declared = declared_version(component, tree)
        .with_context(|| format!("{component}: cannot read the upstream version its tree declares"))?;
    let committed_at = committed_at_utc.ok_or_else(|| {
        anyhow!(
            "{component}: the sync state does not record when its commit was made; \
             run `mattos-build upstream sync {component}` to record it"
        )
    })?;
    let (date, time) = snapshot_timestamp(committed_at)
        .with_context(|| format!("{component}: invalid upstream commit time {committed_at:?}"))?;
    let short = commit.get(..12).unwrap_or(commit);
    Ok(format!("{declared}+git{date}.{time}.{short}"))
}

/// `2026-09-15T14:30:05Z` as (`20260915`, `143005`).
fn snapshot_timestamp(committed_at_utc: &str) -> Result<(String, String)> {
    let time = chrono::DateTime::parse_from_rfc3339(committed_at_utc)?.with_timezone(&chrono::Utc);
    Ok((time.format("%Y%m%d").to_string(), time.format("%H%M%S").to_string()))
}

fn declared_version(component: &str, tree: &Path) -> Result<String> {
    let raw = match declaration(component) {
        Declaration::BuildSystem => {
            if tree.join("meson.build").is_file() {
                meson_project_version(tree)?
            } else if tree.join("configure.ac").is_file() {
                autoconf_version(&read(tree, "configure.ac")?)?
            } else {
                bail!("{} has neither meson.build nor configure.ac", tree.display())
            }
        }
        Declaration::KernelMakefile => {
            let makefile = read(tree, "Makefile")?;
            let [version, patchlevel, sublevel, extra] =
                ["VERSION", "PATCHLEVEL", "SUBLEVEL", "EXTRAVERSION"].map(|name| make_variable(&makefile, name));
            format!(
                "{}.{}.{}{}",
                version.ok_or_else(|| anyhow!("Makefile has no VERSION"))?,
                patchlevel.ok_or_else(|| anyhow!("Makefile has no PATCHLEVEL"))?,
                sublevel.ok_or_else(|| anyhow!("Makefile has no SUBLEVEL"))?,
                extra.unwrap_or_default()
            )
        }
        Declaration::MakeVariables(file, names) => {
            let body = read(tree, file)?;
            join_parts(file, names, |name| make_variable(&body, name))?
        }
        Declaration::CDefines(file, names) => {
            let body = read(tree, file)?;
            join_parts(file, names, |name| c_define(&body, name))?
        }
        Declaration::CString(file) => {
            let body = read(tree, file)?;
            body.split('"').nth(1).map(str::to_string).ok_or_else(|| anyhow!("{file} has no string literal"))?
        }
        Declaration::Cargo(file) => {
            let manifest: toml::Value = toml::from_str(&read(tree, file)?)?;
            manifest
                .get("package")
                .and_then(|package| package.get("version"))
                .and_then(toml::Value::as_str)
                .map(str::to_string)
                .ok_or_else(|| anyhow!("{file} has no [package] version"))?
        }
        Declaration::NcursesVersionFile => {
            let body = read(tree, "VERSION")?;
            let fields = body.split_whitespace().collect::<Vec<_>>();
            match fields.as_slice() {
                [_, release, patch_date, ..] => format!("{release}+{patch_date}"),
                _ => bail!("VERSION is not `<abi> <release> <patch date>`"),
            }
        }
        Declaration::DebianChangelog => {
            let body = read(tree, "debian/changelog")?;
            let head = body.lines().next().unwrap_or_default();
            head.split_once('(')
                .and_then(|(_, rest)| rest.split_once(')'))
                .map(|(version, _)| version.to_string())
                .ok_or_else(|| anyhow!("debian/changelog has no version in its first entry"))?
        }
        Declaration::NewsHeading(prefix) => {
            let body = read(tree, "NEWS")?;
            body.lines()
                .filter_map(|line| line.strip_prefix(prefix))
                .map(|rest| rest.split(|c: char| c == ':' || c.is_whitespace()).next().unwrap_or_default())
                .find(|version| version.starts_with(|c: char| c.is_ascii_digit()))
                .map(str::to_string)
                .ok_or_else(|| anyhow!("NEWS has no `{prefix}<version>` heading"))?
        }
    };
    normalize_declared_version(&raw)
}

fn read(tree: &Path, file: &str) -> Result<String> {
    let path = tree.join(file);
    fs::read_to_string(&path).with_context(|| format!("failed to read {}", path.display()))
}

fn join_parts(file: &str, names: &[&str], value: impl Fn(&str) -> Option<String>) -> Result<String> {
    names
        .iter()
        .map(|name| value(name).ok_or_else(|| anyhow!("{file} does not define {name}")))
        .collect::<Result<Vec<_>>>()
        .map(|parts| parts.join("."))
}

/// A pre-release marker (`1.59.2-dev`, `2.43-devel`, Kbuild's `-rc5`) sorts
/// before its release in Debian only after `~`.
fn normalize_declared_version(raw: &str) -> Result<String> {
    let version = raw.trim().replacen('-', "~", 1);
    if !version.starts_with(|c: char| c.is_ascii_digit())
        || !version.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'+' | b'~'))
    {
        bail!("declared version {raw:?} is not a Debian upstream version");
    }
    Ok(version)
}

fn make_variable(body: &str, name: &str) -> Option<String> {
    body.lines().find_map(|line| {
        let (key, value) = line.split_once('=')?;
        (key.trim() == name).then(|| value.trim().to_string())
    })
}

fn c_define(body: &str, name: &str) -> Option<String> {
    body.lines().find_map(|line| {
        let mut words = line.split_whitespace();
        (words.next()? == "#define" && words.next()? == name).then(|| words.next().map(str::to_string))?
    })
}

/// The `version` keyword argument of meson.build's `project()` call: a
/// string literal, or `files('F')` naming a file that holds it.
fn meson_project_version(tree: &Path) -> Result<String> {
    let body = read(tree, "meson.build")?;
    let start = body.find("project(").ok_or_else(|| anyhow!("meson.build has no project() call"))?;
    let call = balanced_call_arguments(&body[start + "project".len()..])
        .ok_or_else(|| anyhow!("meson.build's project() call is not closed"))?;
    let bytes = call.as_bytes();
    let mut search = 0;
    while let Some(offset) = call[search..].find("version") {
        let at = search + offset;
        search = at + "version".len();
        let preceded_by_identifier = at > 0 && (bytes[at - 1].is_ascii_alphanumeric() || bytes[at - 1] == b'_');
        let rest = call[search..].trim_start();
        let Some(value) = rest.strip_prefix(':').map(str::trim_start) else { continue };
        if preceded_by_identifier {
            continue;
        }
        if let Some(literal) = quoted(value) {
            return Ok(literal);
        }
        if let Some(file) = value.strip_prefix("files(").and_then(quoted) {
            let contents = read(tree, &file)?;
            return contents
                .lines()
                .next()
                .map(|line| line.trim().to_string())
                .filter(|line| !line.is_empty())
                .ok_or_else(|| anyhow!("{file} is empty"));
        }
        bail!("meson.build declares its version with an unsupported expression: {value:.40}");
    }
    bail!("meson.build's project() call declares no version")
}

/// The leading single-quoted string of `text`.
fn quoted(text: &str) -> Option<String> {
    let rest = text.trim_start().strip_prefix('\'')?;
    rest.split_once('\'').map(|(literal, _)| literal.to_string())
}

/// The text between the parentheses opening at the start of `text` (after
/// optional whitespace) and their match, skipping quoted strings.
fn balanced_call_arguments(text: &str) -> Option<&str> {
    let open = text.find('(')?;
    let mut depth = 0usize;
    let mut quote = None;
    for (index, c) in text[open..].char_indices() {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (Some(_), _) => {}
            (None, '\'' | '"') => quote = Some(c),
            (None, '(' | '[') => depth += 1,
            (None, ')' | ']') => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(&text[open + 1..open + index]);
                }
            }
            _ => {}
        }
    }
    None
}

/// The second `AC_INIT` argument, with simple `m4_define` macros resolved.
/// GnuPG projects compose theirs from `mym4_major`, `mym4_minor` and an
/// optional `mym4_micro`.
fn autoconf_version(configure: &str) -> Result<String> {
    let start = configure.find("AC_INIT(").ok_or_else(|| anyhow!("configure.ac has no AC_INIT"))?;
    let arguments = balanced_call_arguments(&configure[start + "AC_INIT".len()..])
        .ok_or_else(|| anyhow!("configure.ac's AC_INIT is not closed"))?;
    let version = split_m4_arguments(arguments)
        .get(1)
        .map(|argument| unquote_m4(argument))
        .ok_or_else(|| anyhow!("AC_INIT has no version argument"))?;
    let defines = m4_defines(configure);
    if version == "mym4_version" && defines.contains_key("mym4_major") {
        let parts = ["mym4_major", "mym4_minor", "mym4_micro"]
            .iter()
            .filter_map(|name| defines.get(*name).cloned())
            .collect::<Vec<_>>();
        return Ok(parts.join("."));
    }
    let mut resolved = version;
    for _ in 0..4 {
        let expanded = resolved
            .split('.')
            .map(|part| defines.get(part).cloned().unwrap_or_else(|| part.to_string()))
            .collect::<Vec<_>>()
            .join(".");
        if expanded == resolved {
            break;
        }
        resolved = expanded;
    }
    Ok(resolved)
}

/// `m4_define([name], [value])` definitions whose value is a plain literal.
fn m4_defines(configure: &str) -> BTreeMap<String, String> {
    let mut defines = BTreeMap::new();
    let mut rest = configure;
    while let Some(start) = rest.find("m4_define(") {
        rest = &rest[start + "m4_define".len()..];
        let Some(arguments) = balanced_call_arguments(rest) else { break };
        if let [name, value] = split_m4_arguments(arguments).as_slice() {
            let value = unquote_m4(value);
            if !value.contains(['(', ')', '[', ']', ',', ' ']) {
                defines.insert(unquote_m4(name), value);
            }
        }
    }
    defines
}

/// Top-level comma-separated arguments of an m4 call, honouring `[` `]`
/// quoting and nested parentheses.
fn split_m4_arguments(arguments: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut depth = 0i32;
    let mut current = String::new();
    for c in arguments.chars() {
        match c {
            '[' | '(' => depth += 1,
            ']' | ')' => depth -= 1,
            ',' if depth == 0 => {
                parts.push(std::mem::take(&mut current));
                continue;
            }
            _ => {}
        }
        current.push(c);
    }
    parts.push(current);
    parts
}

fn unquote_m4(argument: &str) -> String {
    let trimmed = argument.trim();
    trimmed
        .strip_prefix('[')
        .and_then(|inner| inner.strip_suffix(']'))
        .unwrap_or(trimmed)
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree(files: &[(&str, &str)]) -> tempfile::TempDir {
        let temp = tempfile::tempdir().unwrap();
        for (path, body) in files {
            let path = temp.path().join(path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, body).unwrap();
        }
        temp
    }

    fn version(component: &str, files: &[(&str, &str)]) -> String {
        declared_version(component, tree(files).path()).unwrap()
    }

    #[test]
    fn meson_reads_project_version_not_meson_version() {
        let meson = "project('dbus-broker', 'c',\n  meson_version: '>=1.3',\n  version: '37',\n)\n";
        assert_eq!(version("x", &[("meson.build", meson)]), "37");
        let files = "project('mesa', ['c'], version : files('VERSION'), meson_version : '>= 1.4')\n";
        assert_eq!(version("x", &[("meson.build", files), ("VERSION", "26.1.7\n")]), "26.1.7");
        let dev = "project('NetworkManager', 'c', version: '1.59.2-dev')\n";
        assert_eq!(version("x", &[("meson.build", dev)]), "1.59.2~dev");
        let missing = tree(&[("meson.build", "project('x', meson_version: '>=1')\n")]);
        assert!(declared_version("x", missing.path()).is_err());
    }

    #[test]
    fn autoconf_resolves_literal_and_macro_versions() {
        let literal = "AC_INIT([shadow], [4.20.0], [bugs@example.org], [],\n\t[https://example.org])\n";
        assert_eq!(version("x", &[("configure.ac", literal)]), "4.20.0");
        let canberra = "m4_define([ca_major],[0])\nm4_define([ca_minor],[29])\n\
                        m4_define([ca_version],[ca_major.ca_minor])\nAC_INIT([libcanberra], [ca_version], [x])\n";
        assert_eq!(version("x", &[("configure.ac", canberra)]), "0.29");
        let gnupg = "m4_define([mym4_major], [1])\nm4_define([mym4_minor], [12])\nm4_define([mym4_micro], [3])\n\
                     m4_define([mym4_version], m4_argn(4, mym4_verslist))\n\
                     AC_INIT([mym4_package],[mym4_version],[https://bugs.gnupg.org])\n";
        assert_eq!(version("x", &[("configure.ac", gnupg)]), "1.12.3");
        let no_micro = gnupg.replace("m4_define([mym4_micro], [3])\n", "");
        assert_eq!(version("x", &[("configure.ac", &no_micro)]), "1.12");
    }

    #[test]
    fn component_specific_declarations() {
        let kernel = "VERSION = 7\nPATCHLEVEL = 2\nSUBLEVEL = 0\nEXTRAVERSION = -rc5\nNAME = x\n";
        assert_eq!(version("linux-uapi", &[("Makefile", kernel)]), "7.2.0~rc5");
        let release = kernel.replace("-rc5", "");
        assert_eq!(version("linux", &[("Makefile", &release)]), "7.2.0");
        let gmp = "#define __GNU_MP_VERSION            6\n#define __GNU_MP_VERSION_MINOR      3\n\
                   #define __GNU_MP_VERSION_PATCHLEVEL 0\n";
        assert_eq!(version("gmp", &[("gmp-h.in", gmp)]), "6.3.0");
        assert_eq!(version("libcap", &[("Make.Rules", "#\nVERSION=2\nMINOR=78\n")]), "2.78");
        let iproute = "static const char version[] = \"7.1.0\";\n";
        assert_eq!(version("iproute2", &[("include/version.h", iproute)]), "7.1.0");
        assert_eq!(version("ncurses", &[("VERSION", "5:0:10\t6.6\t20260725\n")]), "6.6+20260725");
        let changelog = "pop-fonts (1.0.3) zesty; urgency=medium\n\n  * x\n";
        assert_eq!(version("pop-fonts", &[("debian/changelog", changelog)]), "1.0.3");
        let procps = "procps-ng-NEXT\n---\n\nprocps-ng-4.0.6\n---\n";
        assert_eq!(version("procps-ng", &[("NEWS", procps)]), "4.0.6");
        let util_linux = "util-linux 2.43-devel: Feb 26 2026 (the latest and greatest!)\n";
        assert_eq!(version("util-linux", &[("NEWS", util_linux)]), "2.43~devel");
        let cargo = "[package]\nname = \"greetd\"\nversion = \"0.10.3\"\n";
        assert_eq!(version("greetd", &[("greetd/Cargo.toml", cargo)]), "0.10.3");
    }

    #[test]
    fn snapshot_versions_order_by_declared_version_then_commit_time() {
        let meson = |v: &str| format!("project('p', 'c', version: '{v}')\n");
        let old = tree(&[("meson.build", &meson("1.18.0"))]);
        let new = tree(&[("meson.build", &meson("1.19.0"))]);
        let at = |tree: &tempfile::TempDir, commit: &str, time: &str| {
            snapshot_upstream_version("p", tree.path(), commit, Some(time)).unwrap()
        };
        let first = at(&old, "ffffffffffffffff", "2026-09-15T09:05:00Z");
        assert_eq!(first, "1.18.0+git20260915.090500.ffffffffffff");
        // Later the same day with a lower hash, later day, and a new release.
        let later = [
            at(&old, "0000000000000000", "2026-09-15T14:00:00Z"),
            at(&old, "0000000000000000", "2026-09-16T00:00:00Z"),
            at(&new, "0000000000000000", "2026-01-01T00:00:00Z"),
        ];
        let mut previous = first;
        for version in later {
            let status = std::process::Command::new("dpkg")
                .args(["--compare-versions", &version, "gt", &previous])
                .status()
                .unwrap();
            assert!(status.success(), "{version} must sort after {previous}");
            previous = version;
        }
        // Everything sorts after the retired `0~git.<commit>` form.
        let status = std::process::Command::new("dpkg")
            .args(["--compare-versions", &previous, "gt", "0~git.ffffffffffff"])
            .status()
            .unwrap();
        assert!(status.success());
        let unrecorded = snapshot_upstream_version("p", old.path(), "abc", None).unwrap_err();
        assert!(format!("{unrecorded:#}").contains("mattos-build upstream sync p"));
    }

    /// Every component packaged from a moving-branch snapshot declares a
    /// version this module can read in its vendored tree.
    #[test]
    fn every_packaged_snapshot_component_declares_a_version() {
        let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
        let mut failures = Vec::new();
        for component in PACKAGED_SNAPSHOT_COMPONENTS {
            let state: toml::Value = toml::from_str(
                &fs::read_to_string(repo.join(format!("upstream/state/{component}.toml"))).unwrap(),
            )
            .unwrap();
            let path = state["destination_path"].as_str().unwrap();
            match declared_version(component, &repo.join(path)) {
                Ok(version) => println!("{component}: {version}"),
                Err(error) => failures.push(format!("{component}: {error:#}")),
            }
        }
        assert!(failures.is_empty(), "{failures:#?}");
    }

    const PACKAGED_SNAPSHOT_COMPONENTS: &[&str] = &[
        "dbus-broker", "flatpak", "gmp", "greetd", "iproute2", "iputils", "kmod", "libassuan",
        "libcanberra", "libcap", "libdisplay-info", "libdrm", "libevdev", "libgcrypt",
        "libgpg-error", "libinput", "libksba", "libndp", "linux-uapi", "mesa", "ncurses",
        "networkmanager", "npth", "pixman", "polkit", "pop-fonts", "procps-ng", "seatd", "shadow",
        "systemd", "util-linux",
    ];
}
