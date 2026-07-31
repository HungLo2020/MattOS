use anyhow::{Context, Result, anyhow, bail};
use chrono::Utc;
use clap::{Parser, Subcommand, ValueEnum};
use serde::{Deserialize, Serialize};
use std::ffi::OsStr;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Output};

#[derive(Parser, Debug)]
#[command(name = "mattos-build")]
#[command(about = "MattOS build and upstream orchestration tool")]
struct Cli {
	#[command(subcommand)]
	command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
	Doctor,
	BootstrapWsl {
		#[arg(long, default_value = "Ubuntu")]
		distro: String,
		#[arg(long, default_value = "~/src/MattOS")]
		repo_path: String,
		#[arg(long)]
		skip_package_install: bool,
	},
	BuildWslIso {
		#[arg(long, default_value = "Ubuntu")]
		distro: String,
		#[arg(long, default_value = "~/src/MattOS")]
		repo_path: String,
		#[arg(long)]
		skip_boot_test: bool,
	},
	CopyIsoFromWsl {
		#[arg(long, default_value = "Ubuntu")]
		distro: String,
		#[arg(long, default_value = "~/src/MattOS")]
		repo_path: String,
		#[arg(long)]
		windows_destination: Option<String>,
	},
	BootstrapWindows {
		#[arg(long, default_value = "Ubuntu")]
		distro: String,
		#[arg(long)]
		install_distro: bool,
		#[arg(long)]
		skip_package_install: bool,
	},
	Import {
		#[arg(long)]
		all: bool,
		#[arg(long)]
		component: Option<String>,
		#[arg(long)]
		update: bool,
	},
	Build {
		#[arg(value_enum)]
		stage: BuildStage,
	},
	RunQemu,
}

#[derive(Clone, Debug, ValueEnum)]
enum BuildStage {
	Kernel,
	Brush,
	Coreutils,
	Init,
	Rootfs,
	Initramfs,
	Iso,
	All,
}

#[derive(Debug, Deserialize)]
struct Sources {
	component: Vec<ComponentDef>,
}

#[derive(Debug, Deserialize, Clone)]
struct ComponentDef {
	name: String,
	repo: String,
	branch: String,
	path: String,
	sync: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct SyncState {
	component: String,
	repo: String,
	branch: String,
	imported_commit: String,
	imported_at_utc: String,
	sync_method: String,
	destination_path: String,
}

#[derive(Debug)]
struct WslStatus {
	wsl_installed: bool,
	distros: Vec<String>,
}

fn main() -> Result<()> {
	let cli = Cli::parse();
	let repo_root = std::env::current_dir().context("unable to determine current directory")?;

	match cli.command {
		Commands::Doctor => doctor(),
		Commands::BootstrapWsl {
			distro,
			repo_path,
			skip_package_install,
		} => bootstrap_wsl(&repo_root, &distro, &repo_path, skip_package_install),
		Commands::BuildWslIso {
			distro,
			repo_path,
			skip_boot_test,
		} => build_wsl_iso(&repo_root, &distro, &repo_path, skip_boot_test),
		Commands::CopyIsoFromWsl {
			distro,
			repo_path,
			windows_destination,
		} => copy_iso_from_wsl(&repo_root, &distro, &repo_path, windows_destination.as_deref()),
		Commands::BootstrapWindows {
			distro,
			install_distro,
			skip_package_install,
		} => bootstrap_windows(&distro, install_distro, skip_package_install),
		Commands::Import {
			all,
			component,
			update,
		} => import_sources(&repo_root, all, component, update),
		Commands::Build { stage } => build(&repo_root, stage),
		Commands::RunQemu => run_qemu(&repo_root),
	}
}

fn doctor() -> Result<()> {
	println!("MattOS doctor");

	let mut hard_fail = false;
	let mut warnings = false;

	if cfg!(windows) {
		println!("\n[Windows host requirements]");
		hard_fail |= !check_host_tool("git", true)?;
		hard_fail |= !check_host_tool("cargo", true)?;
		hard_fail |= !check_host_tool("rustc", true)?;
		let wsl_ok = check_host_tool("wsl", true)?;
		hard_fail |= !wsl_ok;

		println!("\n[WSL/Linux build requirements]");
		if wsl_ok {
			let status = detect_wsl_status()?;
			if status.distros.is_empty() {
				hard_fail = true;
				println!("[missing] WSL distro (required)");
				println!("         Install one with: wsl --install -d Ubuntu");
			} else {
				let distro = preferred_distro(&status.distros)
					.ok_or_else(|| anyhow!("unable to select WSL distro"))?;
				println!("Using WSL distro: {distro}");
				for tool in [
					"git",
					"make",
					"gcc",
					"ld",
					"objcopy",
					"cpio",
					"gzip",
					"grub-mkrescue",
					"xorriso",
					"bash",
					"cargo",
					"rustc",
				] {
					hard_fail |= !check_wsl_tool(&distro, tool, true)?;
				}
			}
		}

		println!("\n[Optional QEMU validation]");
		let qemu_host = check_host_tool("qemu-system-x86_64", false)?;
		let qemu_wsl = {
			let status = detect_wsl_status()?;
			if let Some(distro) = preferred_distro(&status.distros) {
				check_wsl_tool(&distro, "qemu-system-x86_64", false)?
			} else {
				false
			}
		};
		if !qemu_host && !qemu_wsl {
			warnings = true;
			println!("[missing] qemu-system-x86_64 in both Windows and WSL (optional)");
		}
	} else {
		println!("\n[Linux host requirements]");
		for tool in [
			"git",
			"cargo",
			"rustc",
			"make",
			"gcc",
			"ld",
			"objcopy",
			"cpio",
			"gzip",
			"grub-mkrescue",
			"xorriso",
			"bash",
		] {
			hard_fail |= !check_host_tool(tool, true)?;
		}

		println!("\n[Optional QEMU validation]");
		if !check_host_tool("qemu-system-x86_64", false)? {
			warnings = true;
		}
	}

	if hard_fail {
		bail!("doctor detected missing required prerequisites")
	}

	if warnings {
		println!("doctor completed with optional warnings");
	} else {
		println!("doctor completed successfully");
	}
	Ok(())
}

fn bootstrap_windows(distro: &str, install_distro: bool, skip_package_install: bool) -> Result<()> {
	if !cfg!(windows) {
		bail!("bootstrap-windows is intended for Windows hosts")
	}

	println!("MattOS Windows bootstrap");
	println!("Preferred distro: {distro}");
	println!(
		"Repository script: tools/bootstrap-wsl.ps1 (run in elevated PowerShell when needed)"
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

	let rust_cmd = "command -v rustup >/dev/null 2>&1 || curl https://sh.rustup.rs -sSf | sh -s -- -y";
	run_wsl_bash(&selected, None, rust_cmd)?;

	println!("Bootstrap completed. Re-run doctor to verify prerequisites.");
	Ok(())
}

fn bootstrap_wsl(repo_root: &Path, preferred: &str, repo_path: &str, skip_package_install: bool) -> Result<()> {
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

		let rust_cmd = "command -v rustup >/dev/null 2>&1 || curl https://sh.rustup.rs -sSf | sh -s -- -y";
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
	println!("Kernel builds from /mnt/* are blocked by mattos-build to avoid NTFS case-collision issues.");
	Ok(())
}

fn build_wsl_iso(repo_root: &Path, preferred: &str, repo_path: &str, skip_boot_test: bool) -> Result<()> {
	if !cfg!(windows) {
		bail!("build-wsl-iso is intended to be run from Windows host")
	}

	let distro = require_wsl_ubuntu(preferred)?;
	let linux_repo = resolve_wsl_repo_path(&distro, repo_path)?;
	sync_repo_to_wsl(repo_root, &distro, &linux_repo)?;

	let repo_expr = shell_escape(&linux_repo);
	let build_cmd = format!(
		"set -euo pipefail; case {0} in /mnt/*) echo 'Refusing to build from Windows-mounted path: ' {0} >&2; exit 12;; esac; cd {0}; source $HOME/.cargo/env 2>/dev/null || true; rm -rf kernel/linux userland/brush userland/coreutils upstream/state; mkdir -p kernel/linux userland/brush userland/coreutils upstream/state; cargo run -p mattos-build -- import --all --update; cargo run -p mattos-build -- build all; test -f out/images/mattos-x86_64.iso",
		repo_expr
	);
	run_wsl_bash(&distro, None, &build_cmd)?;

	if !skip_boot_test {
		let repo_expr = shell_escape(&linux_repo);
		let boot_test = format!(
			"set -euo pipefail; cd {0}; if ! command -v qemu-system-x86_64 >/dev/null 2>&1; then echo 'qemu-system-x86_64 missing in WSL'; exit 22; fi; mkdir -p out/logs; rm -f out/logs/qemu-boot-test.log; (sleep 8; printf 'echo __MATTOS_START__\npwd\nls /\necho MARK_MATTOS\nuname -s\ncat /proc/version\nmkdir -p /tmp/test\ntouch /tmp/test/file\nls /tmp/test\necho __MATTOS_BOOT_OK__\n'; sleep 2) | timeout 180s qemu-system-x86_64 -m 1024 -cdrom out/images/mattos-x86_64.iso -nographic -serial stdio -monitor none -no-reboot -no-shutdown >out/logs/qemu-boot-test.log 2>&1 || true; grep -q '^__MATTOS_START__$' out/logs/qemu-boot-test.log; grep -q '^MARK_MATTOS$' out/logs/qemu-boot-test.log; grep -q '^Linux$' out/logs/qemu-boot-test.log; grep -q '^file$' out/logs/qemu-boot-test.log; grep -q '^__MATTOS_BOOT_OK__$' out/logs/qemu-boot-test.log",
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
		repo_expr,
		wsl_dst_expr
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
		"set -euo pipefail; case {0} in /mnt/*) echo 'Refusing Linux worktree on Windows mount: ' {0} >&2; exit 13;; esac; mkdir -p {0}; rsync -a --delete --exclude 'target/' --exclude 'upstream/.tmp/' --exclude 'kernel/linux/' --exclude 'userland/brush/' --exclude 'userland/coreutils/' --exclude 'upstream/state/' {1}/ {0}/",
		repo_expr,
		source_expr
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

fn check_host_tool(cmd: &str, required: bool) -> Result<bool> {
	let found = command_exists_host(cmd)?;
	if found {
		println!("[ok]      {cmd}");
	} else if required {
		println!("[missing] {cmd} (required)");
	} else {
		println!("[missing] {cmd} (optional)");
	}
	Ok(found)
}

fn command_exists_host(cmd: &str) -> Result<bool> {
	let status = if cfg!(windows) {
		Command::new("where").arg(cmd).status()
	} else {
		Command::new("which").arg(cmd).status()
	}
	.with_context(|| format!("failed to probe tool {cmd}"))?;
	Ok(status.success())
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
	if value.chars().all(|c| c.is_ascii_alphanumeric() || "._/-".contains(c)) {
		return value.to_string();
	}
	format!("'{}'", value.replace('\'', "'\\''"))
}

fn import_sources(repo_root: &Path, all: bool, component: Option<String>, update: bool) -> Result<()> {
	let sources = read_sources(repo_root)?;
	let selected = select_components(&sources.component, all, component)?;

	for comp in selected {
		import_component(repo_root, comp, update)?;
	}

	Ok(())
}

fn read_sources(repo_root: &Path) -> Result<Sources> {
	let path = repo_root.join("upstream/sources.toml");
	let text = fs::read_to_string(&path)
		.with_context(|| format!("failed to read sources file: {}", path.display()))?;
	toml::from_str(&text).context("failed to parse upstream/sources.toml")
}

fn select_components<'a>(
	components: &'a [ComponentDef],
	all: bool,
	component: Option<String>,
) -> Result<Vec<&'a ComponentDef>> {
	if all {
		return Ok(components.iter().collect());
	}

	if let Some(name) = component {
		if let Some(found) = components.iter().find(|c| c.name == name) {
			return Ok(vec![found]);
		}
		bail!("unknown component: {name}");
	}

	bail!("pass --all or --component <name>")
}

fn import_component(repo_root: &Path, comp: &ComponentDef, update: bool) -> Result<()> {
	println!("Importing {} from {} ({})", comp.name, comp.repo, comp.branch);
	validate_component_name(&comp.name)?;
	let destination = resolve_component_destination(repo_root, &comp.path)?;

	fs::create_dir_all(&destination)
		.with_context(|| format!("failed to create destination: {}", destination.display()))?;

	assert_clean_destination(repo_root, &comp.path)?;

	if update {
		if let Some(prior_state) = read_sync_state(repo_root, &comp.name)? {
			if prior_state.repo != comp.repo || prior_state.branch != comp.branch {
				bail!(
					"state mismatch for {} (repo/branch changed); inspect upstream/state/{}.toml",
					comp.name,
					comp.name
				)
			}
			update_component(repo_root, comp, &destination, &prior_state)
		} else if is_scaffold_directory(&destination)? {
			println!(
				"No existing sync state for {}; performing initial import into scaffold directory",
				comp.name
			);
			initial_import_component(repo_root, comp, &destination)
		} else {
			bail!(
				"missing upstream state for {}; run initial import before --update",
				comp.name
			)
		}
	} else {
		initial_import_component(repo_root, comp, &destination)
	}
}

fn is_scaffold_directory(dir: &Path) -> Result<bool> {
	if !dir.exists() {
		return Ok(true);
	}

	for entry in fs::read_dir(dir).with_context(|| format!("failed to read {}", dir.display()))? {
		let entry = entry?;
		let name = entry.file_name();
		if name == OsStr::new(".gitkeep") || name == OsStr::new("README.md") {
			continue;
		}
		return Ok(false);
	}
	Ok(true)
}

fn initial_import_component(repo_root: &Path, comp: &ComponentDef, destination: &Path) -> Result<()> {
	let non_placeholder_entries = fs::read_dir(destination)
		.with_context(|| format!("failed to inspect destination: {}", destination.display()))?
		.filter_map(|e| e.ok())
		.filter(|e| e.file_name() != OsStr::new(".gitkeep"))
		.count();

	if non_placeholder_entries > 0 {
		bail!(
			"destination {} is not empty; rerun with --update",
			destination.display()
		);
	}

	let tmp = prepare_tmp_clone(repo_root, comp)?;
	let commit = run_cmd_capture(&tmp, "git", &["rev-parse", "HEAD"])?;

	clear_directory_contents(destination)?;
	copy_tree_excluding_dotgit(&tmp, destination)?;

	let state = SyncState {
		component: comp.name.clone(),
		repo: comp.repo.clone(),
		branch: comp.branch.clone(),
		imported_commit: commit.trim().to_owned(),
		imported_at_utc: Utc::now().to_rfc3339(),
		sync_method: comp.sync.clone(),
		destination_path: comp.path.clone(),
	};
	write_sync_state(repo_root, &comp.name, &state)?;

	fs::remove_dir_all(&tmp)
		.with_context(|| format!("failed to remove temporary directory: {}", tmp.display()))?;

	println!("Imported {} at commit {}", comp.name, state.imported_commit);
	Ok(())
}

fn update_component(
	repo_root: &Path,
	comp: &ComponentDef,
	destination: &Path,
	prior_state: &SyncState,
) -> Result<()> {
	let tmp_upstream = prepare_tmp_clone(repo_root, comp)?;
	let new_commit = run_cmd_capture(&tmp_upstream, "git", &["rev-parse", "HEAD"])?;

	let old_commit = prior_state.imported_commit.trim();
	run_cmd(
		&tmp_upstream,
		"git",
		&["fetch", "--depth", "1", "origin", old_commit],
	)?;

	let tmp_root = repo_root.join("upstream/.tmp");
	let tmp_merge = tmp_root.join(format!("{}-merge", comp.name));
	if tmp_merge.exists() {
		fs::remove_dir_all(&tmp_merge)
			.with_context(|| format!("failed to clean {}", tmp_merge.display()))?;
	}
	fs::create_dir_all(&tmp_merge)
		.with_context(|| format!("failed to create {}", tmp_merge.display()))?;

	run_cmd(&tmp_merge, "git", &["init"])?;
	run_cmd(
		&tmp_merge,
		"git",
		&[
			"remote",
			"add",
			"upstream",
			tmp_upstream
				.to_str()
				.ok_or_else(|| anyhow!("invalid path: {}", tmp_upstream.display()))?,
		],
	)?;
	run_cmd(&tmp_merge, "git", &["fetch", "upstream", old_commit])?;
	run_cmd(
		&tmp_merge,
		"git",
		&["fetch", "upstream", new_commit.trim()],
	)?;
	run_cmd(
		&tmp_merge,
		"git",
		&["checkout", "-q", "-b", "local", old_commit],
	)?;

	clear_directory_contents(&tmp_merge)?;
	copy_tree_excluding_dotgit(destination, &tmp_merge)?;
	run_cmd(&tmp_merge, "git", &["add", "-A"])?;
	run_cmd(
		&tmp_merge,
		"git",
		&[
			"-c",
			"user.name=MattOS Sync Bot",
			"-c",
			"user.email=syncbot@example.invalid",
			"commit",
			"-m",
			"MattOS local snapshot before upstream sync",
		],
	)?;

	let merge_status = run_cmd_status(
		&tmp_merge,
		"git",
		&["merge", "--no-commit", "--no-ff", new_commit.trim()],
	)?;
	let has_conflicts = merge_status.code() == Some(1);
	if !merge_status.success() && !has_conflicts {
		bail!(
			"sync merge failed unexpectedly with status {}",
			merge_status
				.code()
				.map(|c| c.to_string())
				.unwrap_or_else(|| "unknown".to_string())
		);
	}

	clear_directory_contents(destination)?;
	copy_tree_excluding_dotgit(&tmp_merge, destination)?;

	fs::remove_dir_all(&tmp_upstream)
		.with_context(|| format!("failed to remove {}", tmp_upstream.display()))?;
	fs::remove_dir_all(&tmp_merge)
		.with_context(|| format!("failed to remove {}", tmp_merge.display()))?;

	if has_conflicts {
		bail!(
			"upstream sync for {} produced merge conflicts under {}; resolve conflicts and rerun --update",
			comp.name,
			comp.path
		);
	}

	let state = SyncState {
		component: comp.name.clone(),
		repo: comp.repo.clone(),
		branch: comp.branch.clone(),
		imported_commit: new_commit.trim().to_owned(),
		imported_at_utc: Utc::now().to_rfc3339(),
		sync_method: comp.sync.clone(),
		destination_path: comp.path.clone(),
	};
	write_sync_state(repo_root, &comp.name, &state)?;

	println!("Updated {} to commit {}", comp.name, state.imported_commit);
	Ok(())
}

fn validate_component_name(name: &str) -> Result<()> {
	if name.is_empty() {
		bail!("component name must not be empty")
	}
	if !name
		.chars()
		.all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
	{
		bail!("component name contains unsupported characters: {name}")
	}
	Ok(())
}

fn resolve_component_destination(repo_root: &Path, rel_path: &str) -> Result<PathBuf> {
	let rel = Path::new(rel_path);
	if rel.is_absolute() {
		bail!("component path must be relative: {rel_path}")
	}
	for piece in rel.components() {
		match piece {
			Component::Normal(_) | Component::CurDir => {}
			Component::ParentDir => bail!("component path cannot contain '..': {rel_path}"),
			Component::RootDir | Component::Prefix(_) => {
				bail!("component path has invalid prefix/root: {rel_path}")
			}
		}
	}

	let joined = repo_root.join(rel);
	if !joined.starts_with(repo_root) {
		bail!("component path escapes repository root: {rel_path}")
	}
	Ok(joined)
}

fn assert_clean_destination(repo_root: &Path, rel_path: &str) -> Result<()> {
	let output = run_cmd_output(repo_root, "git", &["status", "--porcelain", "--", rel_path])?;
	if !output.status.success() {
		bail!("failed to inspect git status for path: {rel_path}")
	}
	let text = String::from_utf8(output.stdout).context("git status output was not UTF-8")?;
	let has_non_untracked_changes = text
		.lines()
		.map(str::trim)
		.any(|line| !line.is_empty() && !line.starts_with("?? "));

	if has_non_untracked_changes {
		bail!(
			"destination path {} has uncommitted changes; commit/stash first",
			rel_path
		)
	}
	Ok(())
}

fn read_sync_state(repo_root: &Path, name: &str) -> Result<Option<SyncState>> {
	let path = repo_root.join("upstream/state").join(format!("{name}.toml"));
	if !path.exists() {
		return Ok(None);
	}
	let body = fs::read_to_string(&path)
		.with_context(|| format!("failed to read sync state: {}", path.display()))?;
	let state = toml::from_str::<SyncState>(&body)
		.with_context(|| format!("failed to parse sync state: {}", path.display()))?;
	Ok(Some(state))
}

fn prepare_tmp_clone(repo_root: &Path, comp: &ComponentDef) -> Result<PathBuf> {
	let tmp_base = repo_root.join("upstream/.tmp");
	fs::create_dir_all(&tmp_base).context("failed to create temporary import directory")?;
	let tmp = tmp_base.join(format!("{}-clone", comp.name));
	if tmp.exists() {
		fs::remove_dir_all(&tmp)
			.with_context(|| format!("failed to remove previous temp dir: {}", tmp.display()))?;
	}

	run_cmd(
		repo_root,
		"git",
		&[
			"clone",
			"--depth",
			"1",
			"--branch",
			&comp.branch,
			&comp.repo,
			tmp.to_str().ok_or_else(|| anyhow!("invalid temp path"))?,
		],
	)?;

	Ok(tmp)
}

fn clear_directory_contents(dir: &Path) -> Result<()> {
	if !dir.exists() {
		return Ok(());
	}
	for entry in fs::read_dir(dir).with_context(|| format!("failed to read directory: {}", dir.display()))? {
		let entry = entry?;
		let p = entry.path();
		if p.file_name() == Some(OsStr::new(".git")) {
			continue;
		}
		if p.is_dir() {
			fs::remove_dir_all(&p)
				.with_context(|| format!("failed to remove directory: {}", p.display()))?;
		} else {
			fs::remove_file(&p)
				.with_context(|| format!("failed to remove file: {}", p.display()))?;
		}
	}
	Ok(())
}

fn copy_tree_excluding_dotgit(src: &Path, dst: &Path) -> Result<()> {
	fs::create_dir_all(dst)
		.with_context(|| format!("failed to create copy destination: {}", dst.display()))?;
	for entry in fs::read_dir(src).with_context(|| format!("failed to read source dir: {}", src.display()))? {
		let entry = entry?;
		let from = entry.path();
		let name = entry.file_name();

		if name == OsStr::new(".git") {
			continue;
		}

		let to = dst.join(&name);
		if from.is_dir() {
			copy_tree_excluding_dotgit(&from, &to)?;
		} else {
			fs::copy(&from, &to)
				.with_context(|| format!("failed to copy {} to {}", from.display(), to.display()))?;
		}
	}
	Ok(())
}

fn write_sync_state(repo_root: &Path, name: &str, state: &SyncState) -> Result<()> {
	let dir = repo_root.join("upstream/state");
	fs::create_dir_all(&dir).context("failed to create upstream/state")?;
	let path = dir.join(format!("{name}.toml"));
	let body = toml::to_string_pretty(state).context("failed to serialize sync state")?;
	fs::write(&path, body)
		.with_context(|| format!("failed to write sync state: {}", path.display()))?;
	Ok(())
}

fn build(repo_root: &Path, stage: BuildStage) -> Result<()> {
	match stage {
		BuildStage::Kernel => build_kernel(repo_root),
		BuildStage::Brush => build_brush(repo_root),
		BuildStage::Coreutils => build_coreutils(repo_root),
		BuildStage::Init => build_init(repo_root),
		BuildStage::Rootfs => build_rootfs(repo_root),
		BuildStage::Initramfs => build_initramfs(repo_root),
		BuildStage::Iso => build_iso(repo_root),
		BuildStage::All => {
			build_kernel(repo_root)?;
			build_brush(repo_root)?;
			build_coreutils(repo_root)?;
			build_init(repo_root)?;
			build_rootfs(repo_root)?;
			build_initramfs(repo_root)?;
			build_iso(repo_root)?;
			Ok(())
		}
	}
}

fn build_kernel(repo_root: &Path) -> Result<()> {
	assert_kernel_build_path_safe(repo_root)?;
	let linux = repo_root.join("kernel/linux");
	if !linux.join("Makefile").exists() {
		bail!("kernel source not found in {}; run import first", linux.display());
	}
	run_cmd(&linux, "make", &["defconfig"])?;
	run_cmd(&linux, "make", &["-j", "4"])
}

fn assert_kernel_build_path_safe(repo_root: &Path) -> Result<()> {
	if cfg!(unix) && std::env::var("WSL_DISTRO_NAME").is_ok() {
		let root = repo_root.to_string_lossy();
		if root.starts_with("/mnt/") {
			bail!(
				"refusing kernel build from Windows-mounted path {}. Use Linux filesystem path like ~/src/MattOS",
				repo_root.display()
			)
		}
	}
	Ok(())
}

fn build_brush(repo_root: &Path) -> Result<()> {
	let brush = repo_root.join("userland/brush");
	if !brush.join("Cargo.toml").exists() {
		bail!("brush source not found in {}; run import first", brush.display());
	}
	run_cmd(
		&brush,
		"cargo",
		&["build", "--release", "--target", "x86_64-unknown-linux-musl"],
	)
}

fn build_coreutils(repo_root: &Path) -> Result<()> {
	let coreutils = repo_root.join("userland/coreutils");
	if !coreutils.join("Cargo.toml").exists() {
		bail!(
			"coreutils source not found in {}; run import first",
			coreutils.display()
		);
	}
	run_cmd(
		&coreutils,
		"cargo",
		&["build", "--release", "--target", "x86_64-unknown-linux-musl"],
	)
}

fn build_init(repo_root: &Path) -> Result<()> {
	run_cmd(
		repo_root,
		"cargo",
		&[
			"build",
			"--release",
			"--manifest-path",
			"userland/init/Cargo.toml",
			"--target",
			"x86_64-unknown-linux-musl",
		],
	)
}

fn build_rootfs(repo_root: &Path) -> Result<()> {
	let skeleton = repo_root.join("rootfs/skeleton");
	let out = repo_root.join("build/rootfs");

	if out.exists() {
		fs::remove_dir_all(&out).with_context(|| format!("failed to clean {}", out.display()))?;
	}
	copy_tree_excluding_dotgit(&skeleton, &out)?;

	let init_bin = repo_root.join("target/x86_64-unknown-linux-musl/release/mattos-init");
	if !init_bin.exists() {
		bail!(
			"init binary missing at {}; run build init first",
			init_bin.display()
		);
	}

	fs::create_dir_all(out.join("sbin")).context("failed to create /sbin in rootfs")?;
	fs::copy(&init_bin, out.join("sbin/init")).with_context(|| {
		format!(
			"failed to copy init binary from {} into rootfs",
			init_bin.display()
		)
	})?;

	let brush_candidates = [
		repo_root.join("userland/brush/target/x86_64-unknown-linux-musl/release/brush"),
		repo_root.join("userland/brush/target/release/brush"),
	];
	let brush_bin = brush_candidates.iter().find(|p| p.exists()).cloned();

	if let Some(brush_bin) = brush_bin {
		fs::copy(&brush_bin, out.join("bin/brush")).context("failed to copy brush binary")?;
	} else {
		println!("warning: brush binary not found in musl or native release targets");
	}

	let coreutils_candidates = [
		repo_root.join("userland/coreutils/target/x86_64-unknown-linux-musl/release/coreutils"),
		repo_root.join("userland/coreutils/target/x86_64-unknown-linux-musl/release/uutils"),
		repo_root.join("userland/coreutils/target/release/coreutils"),
		repo_root.join("userland/coreutils/target/release/uutils"),
	];

	let coreutils_multicall = coreutils_candidates.iter().find(|p| p.exists()).cloned();

	if let Some(bin) = coreutils_multicall {
		let dst = out.join("bin/coreutils");
		fs::copy(&bin, &dst).with_context(|| {
			format!("failed to copy coreutils multicall binary from {}", bin.display())
		})?;

		create_coreutils_symlinks(&out)?;
	} else {
		println!("warning: coreutils multicall binary not found");
	}

	Ok(())
}

#[cfg(unix)]
fn create_coreutils_symlinks(rootfs: &Path) -> Result<()> {
	use std::os::unix::fs::symlink;

	let bin = rootfs.join("bin");
	for applet in ["pwd", "ls", "echo", "uname", "cat", "mkdir", "touch"] {
		let link = bin.join(applet);
		if link.exists() {
			fs::remove_file(&link)
				.with_context(|| format!("failed to remove existing symlink {}", link.display()))?;
		}
		symlink("/bin/coreutils", &link)
			.with_context(|| format!("failed to create symlink {}", link.display()))?;
	}
	Ok(())
}

#[cfg(not(unix))]
fn create_coreutils_symlinks(_rootfs: &Path) -> Result<()> {
	println!("warning: coreutils symlink generation skipped on non-Unix host");
	Ok(())
}

fn build_initramfs(repo_root: &Path) -> Result<()> {
	let rootfs = repo_root.join("build/rootfs");
	if !rootfs.exists() {
		bail!("rootfs not found; run build rootfs first");
	}

	fs::create_dir_all(repo_root.join("build")).context("failed to create build directory")?;

	run_cmd(
		&rootfs,
		"bash",
		&[
			"-lc",
			"find . -print0 | cpio --null -ov --format=newc | gzip -9 > ../initramfs.cpio.gz",
		],
	)
}

fn build_iso(repo_root: &Path) -> Result<()> {
	let kernel = repo_root.join("kernel/linux/arch/x86/boot/bzImage");
	if !kernel.exists() {
		bail!(
			"kernel image missing at {}; build kernel first",
			kernel.display()
		);
	}

	let initramfs = repo_root.join("build/initramfs.cpio.gz");
	if !initramfs.exists() {
		bail!("initramfs missing at {}; run build initramfs", initramfs.display());
	}

	let iso_root = repo_root.join("build/iso");
	let grub_dir = iso_root.join("boot/grub");
	fs::create_dir_all(&grub_dir).context("failed to create ISO directory layout")?;

	fs::copy(&kernel, iso_root.join("boot/vmlinuz")).context("failed to stage kernel into ISO tree")?;
	fs::copy(&initramfs, iso_root.join("boot/initramfs.cpio.gz"))
		.context("failed to stage initramfs into ISO tree")?;
	fs::copy(repo_root.join("boot/grub/grub.cfg"), grub_dir.join("grub.cfg"))
		.context("failed to copy grub config")?;

	let out_images = repo_root.join("out/images");
	fs::create_dir_all(&out_images).context("failed to create out/images")?;
	run_cmd(
		repo_root,
		"grub-mkrescue",
		&[
			"-o",
			"out/images/mattos-x86_64.iso",
			"build/iso",
		],
	)
}

fn run_qemu(repo_root: &Path) -> Result<()> {
	let iso = repo_root.join("out/images/mattos-x86_64.iso");
	if !iso.exists() {
		bail!("ISO missing at {}; run build iso first", iso.display());
	}

	run_cmd(
		repo_root,
		"qemu-system-x86_64",
		&[
			"-cdrom",
			iso.to_str().ok_or_else(|| anyhow!("invalid ISO path"))?,
			"-m",
			"1024",
			"-serial",
			"mon:stdio",
			"-boot",
			"d",
		],
	)
}

fn run_cmd(cwd: &Path, program: &str, args: &[&str]) -> Result<()> {
	println!("> {} {}", program, args.join(" "));
	let status = run_cmd_status(cwd, program, args)?;
	if status.success() {
		Ok(())
	} else {
		bail!("command failed with status {status}: {} {}", program, args.join(" "))
	}
}

fn run_cmd_status(cwd: &Path, program: &str, args: &[&str]) -> Result<std::process::ExitStatus> {
	Command::new(program)
		.args(args)
		.current_dir(cwd)
		.status()
		.with_context(|| format!("failed to spawn command: {program}"))
}

fn run_cmd_output(cwd: &Path, program: &str, args: &[&str]) -> Result<Output> {
	Command::new(program)
		.args(args)
		.current_dir(cwd)
		.output()
		.with_context(|| format!("failed to spawn command: {program}"))
}

fn run_cmd_capture(cwd: &Path, program: &str, args: &[&str]) -> Result<String> {
	let output = run_cmd_output(cwd, program, args)?;
	if !output.status.success() {
		bail!(
			"command failed with status {}: {} {}",
			output.status,
			program,
			args.join(" ")
		);
	}
	let text = String::from_utf8(output.stdout).context("stdout was not valid UTF-8")?;
	Ok(text)
}

#[cfg(test)]
mod tests {
	use super::*;

	fn run_ok(cwd: &Path, program: &str, args: &[&str]) {
		let status = Command::new(program)
			.args(args)
			.current_dir(cwd)
			.status()
			.expect("spawn test command");
		assert!(status.success(), "command failed: {program} {}", args.join(" "));
	}

	fn write(path: &Path, body: &str) {
		if let Some(parent) = path.parent() {
			fs::create_dir_all(parent).expect("create parent dirs");
		}
		fs::write(path, body).expect("write file");
	}

	#[test]
	fn path_safety_rejects_parent_dir() {
		let root = std::env::temp_dir().join("mattos-path-safety");
		let result = resolve_component_destination(&root, "../escape");
		assert!(result.is_err());
	}

	#[test]
	fn dirty_tree_protection_rejects_changes() {
		let tmp = tempfile::tempdir().expect("tempdir");
		let root = tmp.path();
		run_ok(root, "git", &["init"]);
		run_ok(root, "git", &["config", "user.name", "Test User"]);
		run_ok(root, "git", &["config", "user.email", "test@example.invalid"]);

		write(&root.join("kernel/linux/README"), "base\n");
		run_ok(root, "git", &["add", "."]);
		run_ok(root, "git", &["commit", "-m", "base"]);

		write(&root.join("kernel/linux/README"), "dirty\n");
		let result = assert_clean_destination(root, "kernel/linux");
		assert!(result.is_err());
	}

	#[test]
	fn metadata_roundtrip_written_to_state_file() {
		let tmp = tempfile::tempdir().expect("tempdir");
		let root = tmp.path();
		let state = SyncState {
			component: "linux".to_string(),
			repo: "https://github.com/torvalds/linux.git".to_string(),
			branch: "master".to_string(),
			imported_commit: "abc123".to_string(),
			imported_at_utc: "2026-01-01T00:00:00Z".to_string(),
			sync_method: "copy".to_string(),
			destination_path: "kernel/linux".to_string(),
		};

		write_sync_state(root, "linux", &state).expect("write state");
		let loaded = read_sync_state(root, "linux").expect("read state").expect("present");
		assert_eq!(loaded.repo, state.repo);
		assert_eq!(loaded.branch, state.branch);
		assert_eq!(loaded.imported_commit, state.imported_commit);
	}

	#[test]
	fn sync_update_produces_conflict_markers() {
		let upstream = tempfile::tempdir().expect("upstream tempdir");
		let upstream_root = upstream.path();
		run_ok(upstream_root, "git", &["init", "-b", "main"]);
		run_ok(upstream_root, "git", &["config", "user.name", "Upstream User"]);
		run_ok(
			upstream_root,
			"git",
			&["config", "user.email", "upstream@example.invalid"],
		);
		write(&upstream_root.join("README"), "base\n");
		run_ok(upstream_root, "git", &["add", "."]);
		run_ok(upstream_root, "git", &["commit", "-m", "base"]);

		let workspace = tempfile::tempdir().expect("workspace tempdir");
		let root = workspace.path();
		run_ok(root, "git", &["init"]);
		run_ok(root, "git", &["config", "user.name", "MattOS User"]);
		run_ok(root, "git", &["config", "user.email", "mattos@example.invalid"]);
		write(&root.join("README.md"), "repo\n");
		run_ok(root, "git", &["add", "."]);
		run_ok(root, "git", &["commit", "-m", "init"]);

		let comp = ComponentDef {
			name: "linux".to_string(),
			repo: upstream_root.to_string_lossy().to_string(),
			branch: "main".to_string(),
			path: "kernel/linux".to_string(),
			sync: "copy".to_string(),
		};
		import_component(root, &comp, false).expect("initial import");
		run_ok(root, "git", &["add", "."]);
		run_ok(root, "git", &["commit", "-m", "import"]);

		write(&root.join("kernel/linux/README"), "local\n");
		run_ok(root, "git", &["add", "kernel/linux/README"]);
		run_ok(root, "git", &["commit", "-m", "local edit"]);

		write(&upstream_root.join("README"), "upstream\n");
		run_ok(upstream_root, "git", &["add", "README"]);
		run_ok(upstream_root, "git", &["commit", "-m", "upstream edit"]);

		let result = import_component(root, &comp, true);
		assert!(result.is_err());

		let merged = fs::read_to_string(root.join("kernel/linux/README")).expect("read merged file");
		assert!(merged.contains("<<<<<<<"));
		assert!(merged.contains(">>>>>>>"));
	}

	#[test]
	fn path_safety_accepts_normal_relative_path() {
		let root = std::env::temp_dir().join("mattos-path-ok");
		let result = resolve_component_destination(&root, "kernel/linux").expect("valid path");
		assert!(result.ends_with(Path::new("kernel/linux")));
	}

	#[test]
	fn component_name_validation_rejects_separators() {
		assert!(validate_component_name("linux").is_ok());
		assert!(validate_component_name("bad/name").is_err());
	}

	#[test]
	fn preferred_distro_chooses_ubuntu_first() {
		let distros = vec!["Debian".to_string(), "Ubuntu-24.04".to_string()];
		let selected = preferred_distro(&distros).expect("selected distro");
		assert_eq!(selected, "Ubuntu-24.04");
	}

	#[test]
	fn shell_escape_quotes_spaces() {
		let escaped = shell_escape("hello world");
		assert_eq!(escaped, "'hello world'");
	}

	#[test]
	fn source_selection_requires_flag() {
		let components = vec![ComponentDef {
			name: "linux".to_string(),
			repo: "x".to_string(),
			branch: "main".to_string(),
			path: "kernel/linux".to_string(),
			sync: "copy".to_string(),
		}];
		let result = select_components(&components, false, None);
		assert!(result.is_err());
	}

	#[test]
	fn clear_directory_keeps_git_dir() {
		let tmp = tempfile::tempdir().expect("tempdir");
		let root = tmp.path();
		fs::create_dir_all(root.join(".git")).expect("create .git dir");
		write(&root.join("file.txt"), "x");
		clear_directory_contents(root).expect("clear");
		assert!(root.join(".git").exists());
		assert!(!root.join("file.txt").exists());
	}

	#[test]
	fn copy_tree_ignores_dotgit() {
		let tmp = tempfile::tempdir().expect("tempdir");
		let src = tmp.path().join("src");
		let dst = tmp.path().join("dst");
		fs::create_dir_all(src.join(".git")).expect("create .git");
		write(&src.join("a.txt"), "a");
		copy_tree_excluding_dotgit(&src, &dst).expect("copy tree");
		assert!(dst.join("a.txt").exists());
		assert!(!dst.join(".git").exists());
	}

	#[test]
	fn sync_state_absent_returns_none() {
		let tmp = tempfile::tempdir().expect("tempdir");
		let root = tmp.path();
		let state = read_sync_state(root, "missing").expect("read state");
		assert!(state.is_none());
	}

	#[test]
	fn no_distro_if_list_empty() {
		let selected = preferred_distro(&[]);
		assert!(selected.is_none());
	}

	#[test]
	fn source_selection_by_component() {
		let components = vec![
			ComponentDef {
				name: "linux".to_string(),
				repo: "x".to_string(),
				branch: "main".to_string(),
				path: "kernel/linux".to_string(),
				sync: "copy".to_string(),
			},
			ComponentDef {
				name: "brush".to_string(),
				repo: "y".to_string(),
				branch: "main".to_string(),
				path: "userland/brush".to_string(),
				sync: "copy".to_string(),
			},
		];
		let selected = select_components(&components, false, Some("brush".to_string()))
			.expect("select component");
		assert_eq!(selected.len(), 1);
		assert_eq!(selected[0].name, "brush");
	}

	#[test]
	fn path_safety_rejects_absolute() {
		let root = std::env::temp_dir().join("mattos-path-absolute");
		let absolute = if cfg!(windows) {
			"C:/absolute/path"
		} else {
			"/absolute/path"
		};
		assert!(resolve_component_destination(&root, absolute).is_err());
	}

	#[test]
	fn validate_component_name_accepts_dash_and_underscore() {
		assert!(validate_component_name("core-utils_1").is_ok());
	}

	#[test]
	fn run_cmd_capture_reads_stdout() {
		let tmp = tempfile::tempdir().expect("tempdir");
		let root = tmp.path();
		let text = if cfg!(windows) {
			run_cmd_capture(root, "cmd", &["/C", "echo", "hello"]).expect("capture")
		} else {
			run_cmd_capture(root, "sh", &["-c", "echo hello"]).expect("capture")
		};
		assert!(text.to_ascii_lowercase().contains("hello"));
	}

	#[test]
	fn selected_all_returns_everything() {
		let components = vec![
			ComponentDef {
				name: "linux".to_string(),
				repo: "x".to_string(),
				branch: "main".to_string(),
				path: "kernel/linux".to_string(),
				sync: "copy".to_string(),
			},
			ComponentDef {
				name: "brush".to_string(),
				repo: "y".to_string(),
				branch: "main".to_string(),
				path: "userland/brush".to_string(),
				sync: "copy".to_string(),
			},
		];
		let selected = select_components(&components, true, None).expect("select all");
		assert_eq!(selected.len(), 2);
	}

	#[test]
	fn shell_escape_leaves_safe_text() {
		let escaped = shell_escape("kernel/linux");
		assert_eq!(escaped, "kernel/linux");
	}

	#[test]
	fn path_safety_rejects_parent_in_middle() {
		let root = std::env::temp_dir().join("mattos-path-middle");
		assert!(resolve_component_destination(&root, "kernel/../linux").is_err());
	}

	#[test]
	fn clear_directory_on_missing_dir_is_ok() {
		let tmp = tempfile::tempdir().expect("tempdir");
		let path = tmp.path().join("missing");
		clear_directory_contents(&path).expect("clear missing");
	}

	#[test]
	fn no_duplicate_component_names_required_for_selection_logic() {
		let components = vec![ComponentDef {
			name: "linux".to_string(),
			repo: "x".to_string(),
			branch: "main".to_string(),
			path: "kernel/linux".to_string(),
			sync: "copy".to_string(),
		}];
		let selected = select_components(&components, false, Some("linux".to_string()))
			.expect("select linux");
		assert_eq!(selected[0].path, "kernel/linux");
	}

	#[test]
	fn read_sources_parses_components() {
		let tmp = tempfile::tempdir().expect("tempdir");
		let root = tmp.path();
		write(
			&root.join("upstream/sources.toml"),
			"[[component]]\nname='linux'\nrepo='https://example.invalid/linux.git'\nbranch='main'\npath='kernel/linux'\nsync='copy'\n",
		);
		let sources = read_sources(root).expect("read sources");
		assert_eq!(sources.component.len(), 1);
		assert_eq!(sources.component[0].name, "linux");
	}

	#[test]
	fn write_sync_state_creates_directory() {
		let tmp = tempfile::tempdir().expect("tempdir");
		let root = tmp.path();
		let state = SyncState {
			component: "brush".to_string(),
			repo: "https://example.invalid/brush.git".to_string(),
			branch: "main".to_string(),
			imported_commit: "def456".to_string(),
			imported_at_utc: "2026-01-01T00:00:00Z".to_string(),
			sync_method: "copy".to_string(),
			destination_path: "userland/brush".to_string(),
		};
		write_sync_state(root, "brush", &state).expect("write state");
		assert!(root.join("upstream/state/brush.toml").exists());
	}

	#[test]
	fn check_name_rejects_empty() {
		assert!(validate_component_name("").is_err());
	}

	#[test]
	fn source_selection_unknown_component_fails() {
		let components = vec![ComponentDef {
			name: "linux".to_string(),
			repo: "x".to_string(),
			branch: "main".to_string(),
			path: "kernel/linux".to_string(),
			sync: "copy".to_string(),
		}];
		let result = select_components(&components, false, Some("missing".to_string()));
		assert!(result.is_err());
	}

	#[test]
	fn shell_escape_handles_quotes() {
		let escaped = shell_escape("a'b");
		assert_eq!(escaped, "'a'\\''b'");
	}

	#[test]
	fn preferred_distro_falls_back_to_first() {
		let distros = vec!["Debian".to_string(), "Arch".to_string()];
		let selected = preferred_distro(&distros).expect("selected distro");
		assert_eq!(selected, "Debian");
	}

	#[test]
	fn resolve_component_destination_joins_path() {
		let root = std::env::temp_dir().join("mattos-path-join");
		let resolved = resolve_component_destination(&root, "userland/brush").expect("resolve");
		assert!(resolved.ends_with("userland/brush"));
	}

	#[test]
	fn source_selection_all_ignores_component_flag() {
		let components = vec![ComponentDef {
			name: "linux".to_string(),
			repo: "x".to_string(),
			branch: "main".to_string(),
			path: "kernel/linux".to_string(),
			sync: "copy".to_string(),
		}];
		let selected = select_components(&components, true, Some("missing".to_string()))
			.expect("select all");
		assert_eq!(selected.len(), 1);
	}

	#[test]
	fn copy_tree_copies_nested_files() {
		let tmp = tempfile::tempdir().expect("tempdir");
		let src = tmp.path().join("src");
		let dst = tmp.path().join("dst");
		write(&src.join("dir/nested.txt"), "nested");
		copy_tree_excluding_dotgit(&src, &dst).expect("copy");
		assert_eq!(
			fs::read_to_string(dst.join("dir/nested.txt")).expect("read nested"),
			"nested"
		);
	}

	#[test]
	fn validate_component_name_rejects_space() {
		assert!(validate_component_name("bad name").is_err());
	}

	#[test]
	fn path_safety_disallows_dotdot_prefix() {
		let root = std::env::temp_dir().join("mattos-path-prefix");
		assert!(resolve_component_destination(&root, "..\\escape").is_err());
	}

	#[test]
	fn read_sync_state_invalid_toml_errors() {
		let tmp = tempfile::tempdir().expect("tempdir");
		let root = tmp.path();
		write(&root.join("upstream/state/linux.toml"), "not=toml=");
		let result = read_sync_state(root, "linux");
		assert!(result.is_err());
	}

	#[test]
	fn source_file_missing_is_error() {
		let tmp = tempfile::tempdir().expect("tempdir");
		let result = read_sources(tmp.path());
		assert!(result.is_err());
	}

	#[test]
	fn kernel_path_guard_allows_non_mnt_path() {
		let tmp = tempfile::tempdir().expect("tempdir");
		let result = assert_kernel_build_path_safe(tmp.path());
		assert!(result.is_ok());
	}

	#[test]
	fn require_wsl_ubuntu_errors_without_wsl_install() {
		if cfg!(windows) {
			let status = detect_wsl_status().expect("status");
			if !status.wsl_installed {
				let result = require_wsl_ubuntu("Ubuntu");
				assert!(result.is_err());
			}
		}
	}

}
