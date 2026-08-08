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
    let version_output = stable_tool_output(&path, &["--version"])?;
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
    let output = Command::new(tool)
        .args(arguments)
        .env("LC_ALL", "C")
        .env("LANG", "C")
        .env("TZ", "UTC")
        .output()
        .with_context(|| format!("failed to inspect tool {}", tool.display()))?;
    if !output.status.success() {
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
