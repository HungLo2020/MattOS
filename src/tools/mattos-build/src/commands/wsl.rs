fn bootstrap_windows(distro: &str, install_distro: bool, skip_package_install: bool) -> Result<()> {
    if !cfg!(windows) {
        bail!("bootstrap-windows is intended for Windows hosts")
    }

    println!("MattOS Windows bootstrap");
    println!("Preferred distro: {distro}");
    println!(
        "Repository script: src/tools/bootstrap-wsl.ps1 (run in elevated PowerShell when needed)"
    );

    let status = detect_wsl_status()?;
    if !status.wsl_installed {
        bail!("WSL is not installed. Run: wsl --install")
    }

    let selected = if status.distros.is_empty() {
        println!("No WSL distribution is installed.");
        if install_distro {
            println!("> wsl --install -d {distro}");
            run_cmd(Path::new("."), "wsl", &["--install", "-d", distro])?;
            println!(
                "If installation required admin approval and did not complete, rerun exactly: wsl --install -d {distro}"
            );
            distro.to_string()
        } else {
            bail!(
                "No WSL distro installed. Install one with: wsl --install -d {}",
                distro
            )
        }
    } else {
        preferred_distro(&status.distros).ok_or_else(|| anyhow!("unable to select WSL distro"))?
    };

    println!("Selected distro: {selected}");
    if skip_package_install {
        println!("Skipping Linux package installation (--skip-package-install)");
        println!("Checking expected WSL tools (non-fatal while package install is skipped):");
        for tool in ["bash", "git", "cargo", "make"] {
            check_wsl_tool(&selected, tool, false)?;
        }
        return Ok(());
    }

    let packages = [
        "build-essential",
        "git",
        "cpio",
        "gzip",
        "xorriso",
        "grub-pc-bin",
        "grub-common",
        "qemu-system-x86",
        "curl",
        "ca-certificates",
        "pkg-config",
        "musl-tools",
    ];

    let pkg_cmd = format!(
        "sudo apt-get update && sudo apt-get install -y {}",
        packages.join(" ")
    );
    run_wsl_bash(&selected, None, &pkg_cmd)?;

    let rust_cmd =
        "command -v rustup >/dev/null 2>&1 || curl https://sh.rustup.rs -sSf | sh -s -- -y";
    run_wsl_bash(&selected, None, rust_cmd)?;

    let mut missing_required = Vec::new();
    println!("Checking required WSL tools after bootstrap:");
    for tool in ["bash", "git", "cargo", "make"] {
        if !check_wsl_tool(&selected, tool, true)? {
            missing_required.push(tool);
        }
    }
    if !missing_required.is_empty() {
        bail!(
            "missing required tools in WSL distro {}: {}",
            selected,
            missing_required.join(", ")
        );
    }

    println!("Bootstrap completed. Re-run doctor to verify prerequisites.");
    Ok(())
}

fn bootstrap_wsl(
    repo_root: &Path,
    preferred: &str,
    repo_path: &str,
    skip_package_install: bool,
) -> Result<()> {
    if !cfg!(windows) {
        bail!("bootstrap-wsl is intended to be run from Windows host")
    }

    println!("MattOS WSL bootstrap");
    let distro = require_wsl_ubuntu(preferred)?;
    println!("Using WSL distro: {distro}");

    if !skip_package_install {
        let packages = [
            "build-essential",
            "git",
            "cpio",
            "gzip",
            "xorriso",
            "grub-pc-bin",
            "grub-common",
            "qemu-system-x86",
            "curl",
            "ca-certificates",
            "pkg-config",
            "musl-tools",
            "bc",
            "bison",
            "flex",
            "libssl-dev",
            "libelf-dev",
            "rsync",
        ];
        let pkg_cmd = format!(
            "sudo apt-get update && sudo apt-get install -y {}",
            packages.join(" ")
        );
        run_wsl_bash(&distro, None, &pkg_cmd)?;

        let rust_cmd =
            "command -v rustup >/dev/null 2>&1 || curl https://sh.rustup.rs -sSf | sh -s -- -y";
        run_wsl_bash(&distro, None, rust_cmd)?;
        run_wsl_bash(
            &distro,
            None,
            "bash -lc 'source $HOME/.cargo/env 2>/dev/null || true; rustup target add x86_64-unknown-linux-musl'",
        )?;
    }

    let linux_repo = resolve_wsl_repo_path(&distro, repo_path)?;
    sync_repo_to_wsl(repo_root, &distro, &linux_repo)?;
    println!("WSL repository is ready at {linux_repo}");
    println!(
        "Kernel builds from /mnt/* are blocked by mattos-build to avoid NTFS case-collision issues."
    );
    Ok(())
}

fn build_wsl_iso(
    repo_root: &Path,
    preferred: &str,
    repo_path: &str,
    skip_boot_test: bool,
) -> Result<()> {
    if !cfg!(windows) {
        bail!("build-wsl-iso is intended to be run from Windows host")
    }

    let distro = require_wsl_ubuntu(preferred)?;
    let linux_repo = resolve_wsl_repo_path(&distro, repo_path)?;
    sync_repo_to_wsl(repo_root, &distro, &linux_repo)?;

    let repo_expr = shell_escape(&linux_repo);
    let build_cmd = format!(
        "set -euo pipefail; case {0} in /mnt/*) echo 'Refusing to build from Windows-mounted path: ' {0} >&2; exit 12;; esac; cd {0}; source $HOME/.cargo/env 2>/dev/null || true; rm -rf src/kernel/linux src/userland/brush src/userland/coreutils src/userland/grep src/userland/sed src/userland/findutils src/userland/diffutils upstream/state; mkdir -p src/kernel/linux src/userland/brush src/userland/coreutils src/userland/grep src/userland/sed src/userland/findutils src/userland/diffutils upstream/state; cargo run -p mattos-build -- import --all --update; cargo run -p mattos-build -- build all; test -f out/images/mattos-x86_64.iso",
        repo_expr
    );
    run_wsl_bash(&distro, None, &build_cmd)?;

    if !skip_boot_test {
        let repo_expr = shell_escape(&linux_repo);
        let boot_test = format!(
            "set -euo pipefail; cd {0}; if ! command -v qemu-system-x86_64 >/dev/null 2>&1; then echo 'qemu-system-x86_64 missing in WSL'; exit 22; fi; mkdir -p out/logs; rm -f out/logs/qemu-boot-test.log; (sleep 8; printf 'echo __MATTOS_START__\npwd\nls /\necho MARK_MATTOS\nuname -s\ncat /proc/version\nmkdir -p /tmp/test\ntouch /tmp/test/file\nls /tmp/test\necho __MATTOS_BOOT_OK__\n'; sleep 2) | timeout 180s qemu-system-x86_64 -m 1024 -drive file=out/images/mattos-x86_64.iso,if=none,id=mattos-cd,media=cdrom,readonly=on -device virtio-scsi-pci,id=mattos-scsi -device scsi-cd,drive=mattos-cd,bus=mattos-scsi.0,bootindex=1 -nographic -serial stdio -monitor none -no-reboot -no-shutdown >out/logs/qemu-boot-test.log 2>&1 || true; grep -q '^__MATTOS_START__$' out/logs/qemu-boot-test.log; grep -q '^MARK_MATTOS$' out/logs/qemu-boot-test.log; grep -q '^Linux$' out/logs/qemu-boot-test.log; grep -q '^file$' out/logs/qemu-boot-test.log; grep -q '^__MATTOS_BOOT_OK__$' out/logs/qemu-boot-test.log",
            repo_expr
        );
        run_wsl_bash(&distro, None, &boot_test)?;
    }

    copy_iso_from_wsl(repo_root, &distro, &linux_repo, None)?;
    println!("WSL build complete; ISO copied to Windows out/images/mattos-x86_64.iso");
    Ok(())
}

fn copy_iso_from_wsl(
    repo_root: &Path,
    preferred: &str,
    repo_path: &str,
    windows_destination: Option<&str>,
) -> Result<()> {
    if !cfg!(windows) {
        bail!("copy-iso-from-wsl is intended to be run from Windows host")
    }

    let distro = require_wsl_ubuntu(preferred)?;
    let linux_repo = resolve_wsl_repo_path(&distro, repo_path)?;

    let windows_dst = if let Some(dst) = windows_destination {
        PathBuf::from(dst)
    } else {
        repo_root.join("out/images/mattos-x86_64.iso")
    };

    if let Some(parent) = windows_dst.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create destination dir {}", parent.display()))?;
    }

    let windows_dst_abs = if windows_dst.is_absolute() {
        windows_dst
    } else {
        repo_root.join(windows_dst)
    };
    let wsl_dst = windows_path_to_wsl(&windows_dst_abs)?;
    let repo_expr = shell_escape(&linux_repo);
    let wsl_dst_expr = shell_escape(&wsl_dst);

    let copy_cmd = format!(
        "set -euo pipefail; test -f {0}/out/images/mattos-x86_64.iso; mkdir -p $(dirname {1}); cp {0}/out/images/mattos-x86_64.iso {1}",
        repo_expr, wsl_dst_expr
    );
    run_wsl_bash(&distro, None, &copy_cmd)?;
    println!("Copied ISO to {}", windows_dst_abs.display());
    Ok(())
}

fn require_wsl_ubuntu(preferred: &str) -> Result<String> {
    let status = detect_wsl_status()?;
    if !status.wsl_installed {
        bail!("WSL is not installed. Run exactly: wsl --install")
    }

    if status.distros.is_empty() {
        bail!("No WSL distro installed. Run exactly (elevated PowerShell): wsl --install -d Ubuntu")
    }

    if status
        .distros
        .iter()
        .any(|d| d.eq_ignore_ascii_case(preferred))
    {
        return Ok(preferred.to_string());
    }

    if let Some(ubuntu) = status
        .distros
        .iter()
        .find(|d| d.to_ascii_lowercase().starts_with("ubuntu"))
    {
        return Ok(ubuntu.clone());
    }

    bail!(
        "Ubuntu WSL distribution not found. Installed distros: {}. Install with: wsl --install -d Ubuntu",
        status.distros.join(", ")
    )
}

fn sync_repo_to_wsl(repo_root: &Path, distro: &str, repo_path: &str) -> Result<()> {
    let source = windows_path_to_wsl(repo_root)?;
    let source_expr = shell_escape(&source);
    let repo_expr = shell_escape(repo_path);
    let cmd = format!(
        "set -euo pipefail; case {0} in /mnt/*) echo 'Refusing Linux worktree on Windows mount: ' {0} >&2; exit 13;; esac; mkdir -p {0}; rsync -a --delete --exclude 'target/' --exclude 'upstream/.tmp/' --exclude 'src/kernel/linux/' --exclude 'src/userland/brush/' --exclude 'src/userland/coreutils/' --exclude 'src/userland/grep/' --exclude 'src/userland/sed/' --exclude 'src/userland/findutils/' --exclude 'src/userland/diffutils/' --exclude 'upstream/state/' {1}/ {0}/",
        repo_expr, source_expr
    );
    run_wsl_bash(distro, None, &cmd)
}

fn resolve_wsl_repo_path(distro: &str, repo_path: &str) -> Result<String> {
    if repo_path == "~" {
        return query_wsl_home(distro);
    }
    if let Some(rest) = repo_path.strip_prefix("~/") {
        let home = query_wsl_home(distro)?;
        return Ok(format!("{home}/{rest}"));
    }
    Ok(repo_path.to_string())
}

fn query_wsl_home(distro: &str) -> Result<String> {
    let output = Command::new("wsl")
        .args(["-d", distro, "--", "bash", "-lc", "printf %s \"$HOME\""])
        .output()
        .with_context(|| format!("failed to query HOME for distro {distro}"))?;
    if !output.status.success() {
        bail!("failed to query WSL HOME for distro {distro}")
    }
    let home = String::from_utf8(output.stdout).context("WSL HOME output was not UTF-8")?;
    if home.trim().is_empty() {
        bail!("WSL HOME resolved to empty path")
    }
    Ok(home.trim().to_string())
}

fn detect_wsl_status() -> Result<WslStatus> {
    let wsl_installed = command_exists_host("wsl")?;
    if !wsl_installed {
        return Ok(WslStatus {
            wsl_installed,
            distros: Vec::new(),
        });
    }

    let output = Command::new("wsl")
        .args(["-l", "-q"])
        .output()
        .context("failed to query WSL distributions")?;

    let mut distros = Vec::new();
    let normalized = decode_wsl_text(&output.stdout).replace('\u{0}', "\n");
    for raw in normalized.lines() {
        let mut line = raw.trim().trim_end_matches('\r').trim().to_string();
        if line.ends_with(" (Default)") {
            line = line.trim_end_matches(" (Default)").to_string();
        }
        if !line.is_empty() {
            distros.push(line);
        }
    }

    Ok(WslStatus {
        wsl_installed,
        distros,
    })
}

fn decode_wsl_text(bytes: &[u8]) -> String {
    let likely_utf16 = bytes.len() >= 2 && bytes.iter().skip(1).step_by(2).any(|b| *b == 0);
    if likely_utf16 && bytes.len() % 2 == 0 {
        let words: Vec<u16> = bytes
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        String::from_utf16_lossy(&words)
    } else {
        String::from_utf8_lossy(bytes).to_string()
    }
}

fn preferred_distro(distros: &[String]) -> Option<String> {
    distros
        .iter()
        .find(|d| d.to_ascii_lowercase().starts_with("ubuntu"))
        .cloned()
        .or_else(|| distros.first().cloned())
}

fn check_host_tool_with_hint(
    cmd: &str,
    required: bool,
    local_path_hint: Option<&str>,
) -> Result<bool> {
    let found = command_exists_host(cmd)?;
    if found {
        println!("[ok]      {cmd}");
    } else if required {
        if let Some(path_hint) = local_path_hint {
            println!("[missing] {cmd} (required; also searched rootless fallback at {path_hint})");
        } else {
            println!("[missing] {cmd} (required)");
        }
    } else {
        if let Some(path_hint) = local_path_hint {
            println!("[missing] {cmd} (optional; also searched rootless fallback at {path_hint})");
        } else {
            println!("[missing] {cmd} (optional)");
        }
    }
    Ok(found)
}

fn command_exists_host(cmd: &str) -> Result<bool> {
    #[cfg(windows)]
    {
        return Command::new("where")
            .arg(cmd)
            .status()
            .with_context(|| format!("failed to probe tool {cmd}"))
            .map(|status| status.success());
    }

    #[cfg(not(windows))]
    {
        let Some(path) = std::env::var_os("PATH") else {
            return Ok(false);
        };
        Ok(command_exists_in_path(cmd, &path))
    }
}

#[cfg(not(windows))]
fn command_exists_in_path(cmd: &str, path: &OsStr) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::env::split_paths(path).any(|directory| {
        let candidate = directory.join(cmd);
        candidate
            .metadata()
            .is_ok_and(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
    })
}

fn check_wsl_tool(distro: &str, cmd: &str, required: bool) -> Result<bool> {
    let check = format!("command -v {} >/dev/null 2>&1", shell_escape(cmd));
    let ok = run_wsl_bash_status(distro, None, &check)?;
    if ok {
        println!("[ok]      {cmd}");
    } else if required {
        println!("[missing] {cmd} (required)");
    } else {
        println!("[missing] {cmd} (optional)");
    }
    Ok(ok)
}

fn run_wsl_bash(distro: &str, cwd: Option<&Path>, cmd: &str) -> Result<()> {
    let status = run_wsl_bash_status_code(distro, cwd, cmd)?;
    if status == 0 {
        Ok(())
    } else {
        bail!("WSL command failed (exit {status}): {cmd}")
    }
}

fn run_wsl_bash_status(distro: &str, cwd: Option<&Path>, cmd: &str) -> Result<bool> {
    Ok(run_wsl_bash_status_code(distro, cwd, cmd)? == 0)
}

fn run_wsl_bash_status_code(distro: &str, cwd: Option<&Path>, cmd: &str) -> Result<i32> {
    let wrapped = if let Some(cwd_path) = cwd {
        let wsl_path = windows_path_to_wsl(cwd_path)?;
        format!("cd {} && {}", shell_escape(&wsl_path), cmd)
    } else {
        cmd.to_string()
    };

    let status = Command::new("wsl")
        .args(["-d", distro, "--", "bash", "-lc", &wrapped])
        .status()
        .with_context(|| format!("failed to run WSL command: {wrapped}"))?;

    Ok(status.code().unwrap_or(1))
}

fn windows_path_to_wsl(path: &Path) -> Result<String> {
    let s = path.to_string_lossy();
    if s.len() >= 2 && s.as_bytes()[1] == b':' {
        let drive = s
            .chars()
            .next()
            .ok_or_else(|| anyhow!("invalid Windows path"))?
            .to_ascii_lowercase();
        let rest = s[2..].replace('\\', "/");
        return Ok(format!("/mnt/{drive}{rest}"));
    }
    bail!("expected Windows absolute path, got {}", path.display())
}

fn shell_escape(value: &str) -> String {
    if value
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || "._/-".contains(c))
    {
        return value.to_string();
    }
    format!("'{}'", value.replace('\'', "'\\''"))
}
