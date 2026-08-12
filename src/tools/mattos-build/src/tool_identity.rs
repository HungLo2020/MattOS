use anyhow::{Context, Result, bail};
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::cache_manifest::ToolIdentity;

pub(crate) fn inspect(
    tool: &str,
    executable_digest: impl FnOnce(&Path) -> Result<String>,
) -> Result<ToolIdentity> {
    let path = resolve_executable(tool)?;
    let version_output = if Path::new(tool).file_name().and_then(OsStr::to_str)
        == Some("unsquashfs")
    {
        // squashfs-tools 4.6.1 prints a valid version and exits 1 for this
        // informational mode. Accept only that documented program-specific
        // behavior; all normal probes remain fail-closed.
        stable_tool_output_inner(&path, version_arguments(tool), true)?
    } else {
        stable_tool_output(&path, version_arguments(tool))?
    };
    let target = if matches!(tool, "gcc" | "g++" | "cc" | "c++") {
        stable_tool_output(&path, &["-dumpmachine"])?
    } else if tool == "rustc" {
        stable_tool_output(&path, &["-vV"])?
            .lines()
            .find_map(|line| line.strip_prefix("host: "))
            .unwrap_or("")
            .to_string()
    } else {
        String::new()
    };
    Ok(ToolIdentity {
        resolved_path: normalize_path(&path),
        executable_sha256: executable_digest(&path)?,
        version: version_output.lines().next().unwrap_or("").to_string(),
        target,
    })
}

fn version_arguments(tool: &str) -> &'static [&'static str] {
    match Path::new(tool).file_name().and_then(OsStr::to_str) {
        // squashfs-tools intentionally uses the historical single-dash form.
        Some("mksquashfs" | "unsquashfs") => &["-version"],
        _ => &["--version"],
    }
}

fn resolve_executable(tool: &str) -> Result<PathBuf> {
    let path = std::env::var_os("PATH");
    resolve_executable_from(tool, path.as_deref())
}

pub(crate) fn resolve_executable_from(tool: &str, path: Option<&OsStr>) -> Result<PathBuf> {
    let supplied = Path::new(tool);
    if supplied.components().count() > 1 {
        return supplied
            .canonicalize()
            .with_context(|| format!("unable to resolve tool {tool}"));
    }
    for directory in std::env::split_paths(path.unwrap_or_default()) {
        let candidate = directory.join(tool);
        if candidate.is_file() {
            return candidate
                .canonicalize()
                .with_context(|| format!("unable to resolve tool {tool}"));
        }
    }
    bail!("tool {tool} was not found on PATH")
}

fn stable_tool_output(tool: &Path, arguments: &[&str]) -> Result<String> {
    stable_tool_output_inner(tool, arguments, false)
}

fn stable_tool_output_inner(
    tool: &Path,
    arguments: &[&str],
    allow_exit_one: bool,
) -> Result<String> {
    let output = Command::new(tool)
        .args(arguments)
        .env("LC_ALL", "C")
        .env("LANG", "C")
        .env("TZ", "UTC")
        .output()
        .with_context(|| format!("failed to inspect tool {}", tool.display()))?;
    if !output.status.success() && !(allow_exit_one && output.status.code() == Some(1)) {
        bail!(
            "tool identity probe failed with {}: {} {}",
            output.status,
            tool.display(),
            arguments.join(" ")
        );
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let selected = if stdout.trim().is_empty() {
        stderr.trim()
    } else {
        stdout.trim()
    };
    Ok(selected.replace('\r', ""))
}

fn normalize_path(path: &Path) -> String {
    path.components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

#[cfg(test)]
mod tests {
    use super::version_arguments;

    #[test]
    fn squashfs_tools_use_their_supported_version_switch() {
        assert_eq!(version_arguments("mksquashfs"), ["-version"]);
        assert_eq!(version_arguments("/usr/bin/unsquashfs"), ["-version"]);
        assert_eq!(version_arguments("gcc"), ["--version"]);
    }
}
