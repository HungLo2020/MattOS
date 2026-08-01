use anyhow::{Context, Result, anyhow, bail};
use chrono::Utc;
use clap::{Parser, Subcommand, ValueEnum};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsStr;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Output};

const AUTHORITATIVE_GRUB_CFG: &str = "src/boot/grub/grub.cfg";
const OBSOLETE_GRUB_CFG_PATHS: &[&str] = &["boot/grub/grub.cfg"];
const GRUB_SYSTEMD_ENTRY: &str = "menuentry \"MattOS (systemd)\"";
const GRUB_RESCUE_ENTRY: &str = "menuentry \"MattOS (rescue init)\"";
const GRUB_SYSTEMD_RDINIT: &str = "rdinit=/usr/lib/systemd/systemd";
const GRUB_RESCUE_RDINIT: &str = "rdinit=/usr/libexec/mattos/rescue-init";
const SAFE_IMPORT_PLACEHOLDER_FILES: &[&str] = &[".gitkeep", "README.md"];
const USERLAND_INVENTORY_PATH: &str = "usr/share/mattos/userland-commands.txt";

const COREUTILS_PROVIDER: &str = "uutils/coreutils";
const GREP_PROVIDER: &str = "uutils/grep";
const SED_PROVIDER: &str = "uutils/sed";
const FINDUTILS_PROVIDER: &str = "uutils/findutils";
const DIFFUTILS_PROVIDER: &str = "uutils/diffutils";
const UTIL_LINUX_PROVIDER: &str = "util-linux";
const LINUX_PAM_PROVIDER: &str = "linux-pam";
const SHADOW_PROVIDER: &str = "shadow";
const SUDO_RS_PROVIDER: &str = "sudo-rs";

const DIFFUTILS_EXPECTED_COMMANDS: &[&str] = &["diff", "cmp", "diff3", "sdiff"];
const DIFFUTILS_AVAILABLE_ALIASES: &[&str] = &["diff", "cmp"];

#[derive(Debug, Clone, Copy)]
struct BinaryInstallSpec {
	provider: &'static str,
	source_rel: &'static str,
	install_name: &'static str,
	command_name: &'static str,
}

const USERLAND_BINARY_INSTALLS: &[BinaryInstallSpec] = &[
	BinaryInstallSpec {
		provider: GREP_PROVIDER,
		source_rel: "src/userland/grep/target/release/grep",
		install_name: "grep",
		command_name: "grep",
	},
	BinaryInstallSpec {
		provider: SED_PROVIDER,
		source_rel: "src/userland/sed/target/release/sed",
		install_name: "sed",
		command_name: "sed",
	},
	BinaryInstallSpec {
		provider: FINDUTILS_PROVIDER,
		source_rel: "src/userland/findutils/target/release/find",
		install_name: "find",
		command_name: "find",
	},
	BinaryInstallSpec {
		provider: FINDUTILS_PROVIDER,
		source_rel: "src/userland/findutils/target/release/xargs",
		install_name: "xargs",
		command_name: "xargs",
	},
	BinaryInstallSpec {
		provider: FINDUTILS_PROVIDER,
		source_rel: "src/userland/findutils/target/release/locate",
		install_name: "locate",
		command_name: "locate",
	},
	BinaryInstallSpec {
		provider: FINDUTILS_PROVIDER,
		source_rel: "src/userland/findutils/target/release/updatedb",
		install_name: "updatedb",
		command_name: "updatedb",
	},
	BinaryInstallSpec {
		provider: DIFFUTILS_PROVIDER,
		source_rel: "src/userland/diffutils/target/release/diffutils",
		install_name: "diffutils",
		command_name: "diffutils",
	},
];

#[derive(Default)]
struct UserlandInventory {
	implemented_upstream: BTreeSet<String>,
	compiled: BTreeSet<String>,
	installed: BTreeSet<String>,
	intentionally_excluded: BTreeSet<String>,
	failed_compatibility: BTreeSet<String>,
}

impl UserlandInventory {
	fn add_implemented(&mut self, provider: &str, command: &str) {
		self.implemented_upstream
			.insert(format!("{provider}:{command}"));
	}

	fn add_compiled(&mut self, provider: &str, command: &str) {
		self.compiled.insert(format!("{provider}:{command}"));
	}

	fn add_installed(&mut self, provider: &str, command: &str) {
		self.installed.insert(format!("{provider}:{command}"));
	}

	fn add_excluded(&mut self, provider: &str, command: &str) {
		self.intentionally_excluded
			.insert(format!("{provider}:{command}"));
	}

	fn add_failed(&mut self, provider: &str, command: &str, reason: &str) {
		self.failed_compatibility
			.insert(format!("{provider}:{command} ({reason})"));
	}
}

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
	Upstream {
		#[command(subcommand)]
		command: UpstreamCommands,
	},
	Build {
		#[arg(value_enum)]
		stage: Option<BuildStage>,
	},
	Image,
	Run,
	Clean {
		#[arg(value_enum)]
		target: Option<CleanTarget>,
	},
	#[command(hide = true)]
	BootstrapWsl {
		#[arg(long, default_value = "Ubuntu")]
		distro: String,
		#[arg(long, default_value = "~/src/MattOS")]
		repo_path: String,
		#[arg(long)]
		skip_package_install: bool,
	},
	#[command(hide = true)]
	BuildWslIso {
		#[arg(long, default_value = "Ubuntu")]
		distro: String,
		#[arg(long, default_value = "~/src/MattOS")]
		repo_path: String,
		#[arg(long)]
		skip_boot_test: bool,
	},
	#[command(hide = true)]
	CopyIsoFromWsl {
		#[arg(long, default_value = "Ubuntu")]
		distro: String,
		#[arg(long, default_value = "~/src/MattOS")]
		repo_path: String,
		#[arg(long)]
		windows_destination: Option<String>,
	},
	#[command(hide = true)]
	BootstrapWindows {
		#[arg(long, default_value = "Ubuntu")]
		distro: String,
		#[arg(long)]
		install_distro: bool,
		#[arg(long)]
		skip_package_install: bool,
	},
	#[command(hide = true)]
	Import {
		#[arg(long)]
		all: bool,
		#[arg(long)]
		component: Option<String>,
		#[arg(long)]
		update: bool,
	},
	#[command(hide = true)]
	RunQemu,
}

#[derive(Subcommand, Debug)]
enum UpstreamCommands {
	Status,
	Import {
		#[arg(long)]
		all: bool,
		component: Option<String>,
	},
	Sync {
		#[arg(long)]
		all: bool,
		component: Option<String>,
	},
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum BuildStage {
	Kernel,
	Brush,
	Coreutils,
	Grep,
	Sed,
	Findutils,
	Diffutils,
	Pam,
	Shadow,
	SudoRs,
	UtilLinux,
	Systemd,
	Init,
	Rootfs,
	Initramfs,
	Iso,
	All,
}

#[derive(Clone, Debug, ValueEnum)]
enum CleanTarget {
	Artifacts,
	Logs,
	Cargo,
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
		Commands::Upstream { command } => upstream_command(&repo_root, command),
		Commands::Build { stage } => build(&repo_root, stage.unwrap_or(BuildStage::All)),
		Commands::Image => build_image(&repo_root),
		Commands::Run => run_qemu(&repo_root),
		Commands::Clean { target } => clean(&repo_root, target.unwrap_or(CleanTarget::Artifacts)),
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
		Commands::RunQemu => run_qemu(&repo_root),
	}
}

fn doctor() -> Result<()> {
	println!("MattOS doctor");

	if cfg!(windows) {
		bail!("MattOS build is Linux-native for this milestone; run doctor from Linux filesystem")
	}

	let mut missing_required = Vec::new();
	let mut broken_required = Vec::new();
	let mut missing_optional = Vec::new();
	let mut broken_optional = Vec::new();

	println!("\n[Required tools]");
	let local_tools = local_tool_env(&std::env::current_dir().context("cwd")?);
	let local_path_hint = local_tools
		.as_ref()
		.map(|e| e.tool_bin_dir.display().to_string());
	for tool in [
		"git",
		"cargo",
		"rustc",
		"make",
		"gcc",
		"autoreconf",
		"meson",
		"ninja",
		"gperf",
		"ld",
		"objcopy",
		"perl",
		"python3",
		"bc",
		"cpio",
		"gzip",
		"mformat",
		"mcopy",
		"grub-mkrescue",
		"xorriso",
		"pkg-config",
		"bash",
		"bison",
		"flex",
	] {
		if !check_host_tool_with_hint(tool, true, local_path_hint.as_deref())? {
			missing_required.push(tool);
		}
	}

	for (tool, args) in [
		("mformat", vec!["-V"]),
		("mcopy", vec!["-V"]),
		("meson", vec!["--version"]),
		("ninja", vec!["--version"]),
		("grub-mkrescue", vec!["--version"]),
		("xorriso", vec!["-version"]),
	] {
		if missing_required.contains(&tool) {
			continue;
		}
		if let Some(message) = check_tool_runtime(tool, &args)? {
			println!("[broken]  {tool} ({message})");
			broken_required.push(tool);
		}
	}

	if let Some(message) = check_tool_runtime("python3", &["-c", "import jinja2"])? {
		println!("[broken]  python3-jinja2 ({message})");
		broken_required.push("python3-jinja2");
	}

	if let Some(message) = check_tool_runtime("pkg-config", &["--exists", "mount"])? {
		println!("[broken]  libmount-dev ({message})");
		broken_required.push("libmount-dev");
	}

	println!("\n[Optional tools]");
	for tool in ["qemu-system-x86_64", "clang"] {
		if !check_host_tool_with_hint(tool, false, local_path_hint.as_deref())? {
			missing_optional.push(tool);
		}
	}

	for (tool, args) in [("qemu-system-x86_64", vec!["--version"])] {
		if missing_optional.contains(&tool) {
			continue;
		}
		if let Some(message) = check_tool_runtime(tool, &args)? {
			println!("[broken]  {tool} ({message})");
			broken_optional.push(tool);
		}
	}

	let mut required_issues: Vec<&str> = Vec::new();
	required_issues.extend(missing_required.iter().copied());
	required_issues.extend(broken_required.iter().copied());
	required_issues.sort_unstable();
	required_issues.dedup();

	let mut optional_issues: Vec<&str> = Vec::new();
	optional_issues.extend(missing_optional.iter().copied());
	optional_issues.extend(broken_optional.iter().copied());
	optional_issues.sort_unstable();
	optional_issues.dedup();

	if !required_issues.is_empty() || !optional_issues.is_empty() {
		println!("\n[Suggested packages]");
		if let Some(cmd) = suggested_package_command(&required_issues, &optional_issues)? {
			println!("{cmd}");
		} else {
			println!("No package manager hint available; install missing tools manually.");
		}
	}

	if !missing_required.is_empty() {
		println!("\n[Required missing tools] {}", missing_required.join(", "));
	}
	if !broken_required.is_empty() {
		println!("[Required broken tools] {}", broken_required.join(", "));
	}

	if !missing_required.is_empty() || !broken_required.is_empty() {
		bail!("doctor detected missing or broken required prerequisites")
	}

	if !missing_optional.is_empty() || !broken_optional.is_empty() {
		println!("doctor completed with optional warnings");
	} else {
		println!("doctor completed successfully");
	}
	Ok(())
}

fn upstream_command(repo_root: &Path, command: UpstreamCommands) -> Result<()> {
	match command {
		UpstreamCommands::Status => upstream_status(repo_root),
		UpstreamCommands::Import { all, component } => import_sources(repo_root, all, component, false),
		UpstreamCommands::Sync { all, component } => import_sources(repo_root, all, component, true),
	}
}

fn upstream_status(repo_root: &Path) -> Result<()> {
	let sources = read_sources(repo_root)?;
	println!("MattOS upstream status");
	for comp in &sources.component {
		let destination = resolve_component_destination(repo_root, &comp.path)?;
		let exists = destination.join(".").exists();
		println!("\ncomponent: {}", comp.name);
		println!("  repo:      {}", comp.repo);
		println!("  branch:    {}", comp.branch);
		println!("  path:      {}", comp.path);
		println!("  present:   {}", if exists { "yes" } else { "no" });

		if let Some(state) = read_sync_state(repo_root, &comp.name)? {
			println!("  commit:    {}", state.imported_commit);
			println!("  imported:  {}", state.imported_at_utc);
		} else {
			println!("  commit:    <not imported>");
		}
	}
	Ok(())
}

fn build_image(repo_root: &Path) -> Result<()> {
	build_rootfs(repo_root)?;
	build_initramfs(repo_root)?;
	build_iso(repo_root)
}

fn clean(repo_root: &Path, target: CleanTarget) -> Result<()> {
	match target {
		CleanTarget::Artifacts => {
			remove_path_if_exists(&repo_root.join("out/build"))?;
			remove_path_if_exists(&repo_root.join("out/images"))?;
		}
		CleanTarget::Logs => {
			remove_path_if_exists(&repo_root.join("out/logs"))?;
		}
		CleanTarget::Cargo => {
			remove_path_if_exists(&repo_root.join("target"))?;
			remove_path_if_exists(&repo_root.join("src/userland/brush/target"))?;
			remove_path_if_exists(&repo_root.join("src/userland/coreutils/target"))?;
			remove_path_if_exists(&repo_root.join("src/userland/grep/target"))?;
			remove_path_if_exists(&repo_root.join("src/userland/sed/target"))?;
			remove_path_if_exists(&repo_root.join("src/userland/findutils/target"))?;
			remove_path_if_exists(&repo_root.join("src/userland/diffutils/target"))?;
			remove_path_if_exists(&repo_root.join("src/system/auth/sudo-rs/target"))?;
		}
		CleanTarget::All => {
			remove_path_if_exists(&repo_root.join("out"))?;
			remove_path_if_exists(&repo_root.join("target"))?;
			remove_path_if_exists(&repo_root.join("src/userland/brush/target"))?;
			remove_path_if_exists(&repo_root.join("src/userland/coreutils/target"))?;
			remove_path_if_exists(&repo_root.join("src/userland/grep/target"))?;
			remove_path_if_exists(&repo_root.join("src/userland/sed/target"))?;
			remove_path_if_exists(&repo_root.join("src/userland/findutils/target"))?;
			remove_path_if_exists(&repo_root.join("src/userland/diffutils/target"))?;
			remove_path_if_exists(&repo_root.join("src/system/auth/sudo-rs/target"))?;
			remove_path_if_exists(&repo_root.join("upstream/.tmp"))?;
		}
	}

	println!("cleaned target: {target:?}");
	Ok(())
}

fn remove_path_if_exists(path: &Path) -> Result<()> {
	if path.exists() {
		if path.is_dir() {
			fs::remove_dir_all(path)
				.with_context(|| format!("failed to remove directory {}", path.display()))?;
		} else {
			fs::remove_file(path)
				.with_context(|| format!("failed to remove file {}", path.display()))?;
		}
	}
	Ok(())
}

fn suggested_package_command(required: &[&str], optional: &[&str]) -> Result<Option<String>> {
	let os_release = fs::read_to_string("/etc/os-release").unwrap_or_default();
	let mut all_tools: Vec<&str> = required.iter().chain(optional.iter()).copied().collect();
	all_tools.sort_unstable();
	all_tools.dedup();

	let mut package_list: Vec<&str> = Vec::new();
	for tool in all_tools {
		for pkg in packages_for_tool(tool, &os_release) {
			if !package_list.contains(&pkg) {
				package_list.push(pkg);
			}
		}
	}

	let package_list = package_list.join(" ");

	if os_release.contains("ID=ubuntu") || os_release.contains("ID=debian") {
		return Ok(Some(format!("sudo apt update && sudo apt install -y {package_list}")));
	}
	if os_release.contains("ID=fedora") || os_release.contains("ID=centos") || os_release.contains("ID=rhel") {
		return Ok(Some(format!("sudo dnf install -y {package_list}")));
	}
	if os_release.contains("ID=arch") || os_release.contains("ID_LIKE=arch") {
		return Ok(Some(format!("sudo pacman -S --needed {package_list}")));
	}

	Ok(None)
}

fn packages_for_tool<'a>(tool: &'a str, os_release: &str) -> Vec<&'a str> {
	if os_release.contains("ID=ubuntu") || os_release.contains("ID=debian") {
		return match tool {
			"grub-mkrescue" => vec!["grub-pc-bin", "grub-common"],
			"mformat" | "mcopy" => vec!["mtools"],
			"qemu-system-x86_64" => vec!["qemu-system-x86"],
			"ninja" => vec!["ninja-build"],
			"autoreconf" => vec!["autoconf", "automake", "libtool"],
			"python3-jinja2" => vec!["python3-jinja2"],
			_ => vec![tool],
		};
	}

	if os_release.contains("ID=fedora") || os_release.contains("ID=centos") || os_release.contains("ID=rhel") {
		return match tool {
			"grub-mkrescue" => vec!["grub2-tools"],
			"mformat" | "mcopy" => vec!["mtools"],
			"qemu-system-x86_64" => vec!["qemu-system-x86"],
			"python3-jinja2" => vec!["python3-jinja2"],
			_ => vec![tool],
		};
	}

	if os_release.contains("ID=arch") || os_release.contains("ID_LIKE=arch") {
		return match tool {
			"grub-mkrescue" => vec!["grub"],
			"mformat" | "mcopy" => vec!["mtools"],
			"python3-jinja2" => vec!["python-jinja"],
			_ => vec![tool],
		};
	}

	vec![tool]
}

fn check_tool_runtime(cmd: &str, args: &[&str]) -> Result<Option<String>> {
	let output = Command::new(cmd)
		.args(args)
		.output()
		.with_context(|| format!("failed to execute tool check: {cmd} {}", args.join(" ")))?;

	if output.status.success() {
		return Ok(None);
	}

	let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
	let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
	let detail = if !stderr.is_empty() {
		stderr
	} else if !stdout.is_empty() {
		stdout
	} else {
		format!("exit status {}", output.status)
	};

	Ok(Some(detail))
}

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

	let rust_cmd = "command -v rustup >/dev/null 2>&1 || curl https://sh.rustup.rs -sSf | sh -s -- -y";
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
		"set -euo pipefail; case {0} in /mnt/*) echo 'Refusing to build from Windows-mounted path: ' {0} >&2; exit 12;; esac; cd {0}; source $HOME/.cargo/env 2>/dev/null || true; rm -rf src/kernel/linux src/userland/brush src/userland/coreutils src/userland/grep src/userland/sed src/userland/findutils src/userland/diffutils upstream/state; mkdir -p src/kernel/linux src/userland/brush src/userland/coreutils src/userland/grep src/userland/sed src/userland/findutils src/userland/diffutils upstream/state; cargo run -p mattos-build -- import --all --update; cargo run -p mattos-build -- build all; test -f out/images/mattos-x86_64.iso",
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
		"set -euo pipefail; case {0} in /mnt/*) echo 'Refusing Linux worktree on Windows mount: ' {0} >&2; exit 13;; esac; mkdir -p {0}; rsync -a --delete --exclude 'target/' --exclude 'upstream/.tmp/' --exclude 'src/kernel/linux/' --exclude 'src/userland/brush/' --exclude 'src/userland/coreutils/' --exclude 'src/userland/grep/' --exclude 'src/userland/sed/' --exclude 'src/userland/findutils/' --exclude 'src/userland/diffutils/' --exclude 'upstream/state/' {1}/ {0}/",
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

fn check_host_tool_with_hint(cmd: &str, required: bool, local_path_hint: Option<&str>) -> Result<bool> {
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
		if is_safe_placeholder_entry(&entry)? {
			continue;
		}
		return Ok(false);
	}
	Ok(true)
}

fn is_safe_placeholder_entry(entry: &fs::DirEntry) -> Result<bool> {
	let name = entry.file_name();
	if !SAFE_IMPORT_PLACEHOLDER_FILES
		.iter()
		.any(|allowed| name == OsStr::new(allowed))
	{
		return Ok(false);
	}
	let meta = entry
		.file_type()
		.with_context(|| format!("failed to inspect placeholder type for {}", entry.path().display()))?;
	Ok(meta.is_file())
}

fn initial_import_component(repo_root: &Path, comp: &ComponentDef, destination: &Path) -> Result<()> {
	assert_initial_destination_safe(destination)?;

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

fn assert_initial_destination_safe(destination: &Path) -> Result<()> {
	if !destination.exists() {
		return Ok(());
	}

	let mut unsafe_entries = Vec::new();
	for entry in fs::read_dir(destination)
		.with_context(|| format!("failed to inspect destination: {}", destination.display()))?
	{
		let entry = entry?;
		if is_safe_placeholder_entry(&entry)? {
			continue;
		}
		unsafe_entries.push(
			entry
				.file_name()
				.to_string_lossy()
				.to_string(),
		);
	}

	if !unsafe_entries.is_empty() {
		unsafe_entries.sort();
		bail!(
			"initial import refused: destination {} contains non-placeholder files: {}",
			destination.display(),
			unsafe_entries.join(", ")
		)
	}

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
	if rel_path.contains('\\') {
		bail!("component path must use forward slashes only: {rel_path}")
	}

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
		let metadata = fs::symlink_metadata(&from)
			.with_context(|| format!("failed to read metadata: {}", from.display()))?;

		if name == OsStr::new(".git") {
			continue;
		}

		let to = dst.join(&name);
		if metadata.file_type().is_symlink() {
			copy_symlink(&from, &to)?;
		} else if metadata.is_dir() {
			copy_tree_excluding_dotgit(&from, &to)?;
		} else {
			fs::copy(&from, &to)
				.with_context(|| format!("failed to copy {} to {}", from.display(), to.display()))?;
			preserve_permissions(&metadata, &to)?;
		}
	}
	Ok(())
}

#[cfg(unix)]
fn copy_symlink(from: &Path, to: &Path) -> Result<()> {
	use std::os::unix::fs::symlink;

	if to.exists() {
		fs::remove_file(to).with_context(|| format!("failed to remove {}", to.display()))?;
	}
	let target = fs::read_link(from)
		.with_context(|| format!("failed to read symlink {}", from.display()))?;
	symlink(&target, to)
		.with_context(|| format!("failed to create symlink {}", to.display()))?;
	Ok(())
}

#[cfg(not(unix))]
fn copy_symlink(from: &Path, to: &Path) -> Result<()> {
	let target = fs::read_link(from)
		.with_context(|| format!("failed to read symlink {}", from.display()))?;
	let parent = to
		.parent()
		.ok_or_else(|| anyhow!("missing parent for {}", to.display()))?;
	fs::create_dir_all(parent)
		.with_context(|| format!("failed to create parent {}", parent.display()))?;
	let resolved = from
		.parent()
		.ok_or_else(|| anyhow!("missing parent for {}", from.display()))?
		.join(target);
	fs::copy(&resolved, to)
		.with_context(|| format!("failed to copy symlink fallback {}", resolved.display()))?;
	Ok(())
}

#[cfg(unix)]
fn preserve_permissions(metadata: &fs::Metadata, to: &Path) -> Result<()> {
	use std::os::unix::fs::PermissionsExt;

	let mode = metadata.permissions().mode();
	fs::set_permissions(to, fs::Permissions::from_mode(mode))
		.with_context(|| format!("failed to set permissions on {}", to.display()))?;
	Ok(())
}

#[cfg(not(unix))]
fn preserve_permissions(_metadata: &fs::Metadata, _to: &Path) -> Result<()> {
	Ok(())
}

fn write_sync_state(repo_root: &Path, name: &str, state: &SyncState) -> Result<()> {
	let dir = repo_root.join("upstream/state");
	fs::create_dir_all(&dir).context("failed to create upstream/state")?;
	let path = dir.join(format!("{name}.toml"));
	let temp_path = dir.join(format!("{name}.toml.tmp"));
	let body = toml::to_string_pretty(state).context("failed to serialize sync state")?;
	fs::write(&temp_path, body)
		.with_context(|| format!("failed to write temporary sync state: {}", temp_path.display()))?;
	fs::rename(&temp_path, &path)
		.with_context(|| format!("failed to publish sync state: {}", path.display()))?;
	Ok(())
}

fn build(repo_root: &Path, stage: BuildStage) -> Result<()> {
	for next in build_plan(stage) {
		build_stage(repo_root, next)?;
	}
	Ok(())
}

fn build_plan(stage: BuildStage) -> Vec<BuildStage> {
	if stage == BuildStage::All {
		return vec![
			BuildStage::Kernel,
			BuildStage::Brush,
			BuildStage::Coreutils,
			BuildStage::Grep,
			BuildStage::Sed,
			BuildStage::Findutils,
			BuildStage::Diffutils,
			BuildStage::Pam,
			BuildStage::UtilLinux,
			BuildStage::Shadow,
			BuildStage::SudoRs,
			BuildStage::Systemd,
			BuildStage::Init,
			BuildStage::Rootfs,
			BuildStage::Initramfs,
			BuildStage::Iso,
		];
	}

	vec![stage]
}

fn build_stage(repo_root: &Path, stage: BuildStage) -> Result<()> {
	match stage {
		BuildStage::Kernel => build_kernel(repo_root),
		BuildStage::Brush => build_brush(repo_root),
		BuildStage::Coreutils => build_coreutils(repo_root),
		BuildStage::Grep => build_grep(repo_root),
		BuildStage::Sed => build_sed(repo_root),
		BuildStage::Findutils => build_findutils(repo_root),
		BuildStage::Diffutils => build_diffutils(repo_root),
		BuildStage::Pam => build_linux_pam(repo_root),
		BuildStage::Shadow => build_shadow(repo_root),
		BuildStage::SudoRs => build_sudo_rs(repo_root),
		BuildStage::UtilLinux => build_util_linux(repo_root),
		BuildStage::Systemd => build_systemd(repo_root),
		BuildStage::Init => build_init(repo_root),
		BuildStage::Rootfs => build_rootfs(repo_root),
		BuildStage::Initramfs => build_initramfs(repo_root),
		BuildStage::Iso => build_iso(repo_root),
		BuildStage::All => bail!("internal error: BuildStage::All should be expanded by build_plan"),
	}
}

fn build_kernel(repo_root: &Path) -> Result<()> {
	assert_kernel_build_path_safe(repo_root)?;
	let linux = repo_root.join("src/kernel/linux");
	let config = repo_root.join("src/kernel/config/x86_64_mattos.config");
	if !linux.join("Makefile").exists() {
		bail!("kernel source not found in {}; run import first", linux.display());
	}
	if !config.exists() {
		bail!("kernel config missing at {}; add configuration first", config.display());
	}

	let config_text = fs::read_to_string(&config)
		.with_context(|| format!("failed to read {}", config.display()))?;
	fs::write(linux.join(".config"), config_text)
		.with_context(|| format!("failed to stage kernel config from {}", config.display()))?;

	let env = local_tool_env(repo_root);
	if let Some(env) = &env {
		println!("Using local rootless toolchain from {}", env.tool_root.display());
	}
	run_cmd_with_env(&linux, "make", &["olddefconfig"], env.as_ref())?;
	run_cmd_with_env(&linux, "make", &["-j", "4"], env.as_ref())
		.context("kernel build failed")?;

	let bz = linux.join("arch/x86/boot/bzImage");
	if !bz.exists() {
		bail!("kernel build finished without bzImage at {}", bz.display())
	}
	Ok(())

	
}

#[derive(Debug, Clone)]
struct LocalToolEnv {
	tool_root: PathBuf,
	tool_bin_dir: PathBuf,
	tool_lib_dir: PathBuf,
	tool_include_dir: PathBuf,
	bison_pkg_data_dir: PathBuf,
	m4_bin: PathBuf,
}

fn local_tool_env(repo_root: &Path) -> Option<LocalToolEnv> {
	let root = repo_root.join(".tools/rootless/usr");
	let bin = root.join("bin");
	let lib = root.join("lib/x86_64-linux-gnu");
	let include = root.join("include");
	let bison_pkg = root.join("share/bison");
	let m4 = bin.join("m4");
	if bin.exists() && lib.exists() && include.exists() && bison_pkg.exists() && m4.exists() {
		Some(LocalToolEnv {
			tool_root: root,
			tool_bin_dir: bin,
			tool_lib_dir: lib,
			tool_include_dir: include,
			bison_pkg_data_dir: bison_pkg,
			m4_bin: m4,
		})
	} else {
		None
	}
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
	let brush = repo_root.join("src/userland/brush");
	if !brush.join("Cargo.toml").exists() {
		bail!("brush source not found in {}; run import first", brush.display());
	}
	run_cmd(&brush, "cargo", &["build", "--release", "-p", "brush"])
}

fn build_coreutils(repo_root: &Path) -> Result<()> {
	let coreutils = repo_root.join("src/userland/coreutils");
	if !coreutils.join("Cargo.toml").exists() {
		bail!(
			"coreutils source not found in {}; run import first",
			coreutils.display()
		);
	}
	run_cmd(
		&coreutils,
		"cargo",
		&[
			"build",
			"--release",
			"-p",
			"coreutils",
			"--no-default-features",
			"--features",
			"unix",
		],
	)
}

fn build_grep(repo_root: &Path) -> Result<()> {
	let grep = repo_root.join("src/userland/grep");
	if !grep.join("Cargo.toml").exists() {
		bail!("grep source not found in {}; run import first", grep.display());
	}
	run_cmd(
		repo_root,
		"cargo",
		&[
			"build",
			"--release",
			"--manifest-path",
			"src/userland/grep/Cargo.toml",
			"--bin",
			"grep",
		],
	)
}

fn build_sed(repo_root: &Path) -> Result<()> {
	let sed = repo_root.join("src/userland/sed");
	if !sed.join("Cargo.toml").exists() {
		bail!("sed source not found in {}; run import first", sed.display());
	}
	run_cmd(
		repo_root,
		"cargo",
		&[
			"build",
			"--release",
			"--manifest-path",
			"src/userland/sed/Cargo.toml",
			"--bin",
			"sed",
		],
	)
}

fn build_findutils(repo_root: &Path) -> Result<()> {
	let findutils = repo_root.join("src/userland/findutils");
	if !findutils.join("Cargo.toml").exists() {
		bail!(
			"findutils source not found in {}; run import first",
			findutils.display()
		);
	}
	run_cmd(
		repo_root,
		"cargo",
		&[
			"build",
			"--release",
			"--manifest-path",
			"src/userland/findutils/Cargo.toml",
			"--bins",
		],
	)
}

fn build_diffutils(repo_root: &Path) -> Result<()> {
	let diffutils = repo_root.join("src/userland/diffutils");
	if !diffutils.join("Cargo.toml").exists() {
		bail!(
			"diffutils source not found in {}; run import first",
			diffutils.display()
		);
	}
	run_cmd(
		repo_root,
		"cargo",
		&[
			"build",
			"--release",
			"--manifest-path",
			"src/userland/diffutils/Cargo.toml",
			"--bin",
			"diffutils",
		],
	)
}

fn build_init(repo_root: &Path) -> Result<()> {
	run_cmd(
		repo_root,
		"cargo",
		&["build", "--release", "--manifest-path", "src/userland/init/Cargo.toml"],
	)
}

fn build_linux_pam(repo_root: &Path) -> Result<()> {
	let pam_src = repo_root.join("src/system/auth/linux-pam");
	if !pam_src.join("meson.build").exists() {
		bail!(
			"linux-pam source not found in {}; run upstream import linux-pam first",
			pam_src.display()
		);
	}

	let out_root = repo_root.join("out/build/linux-pam");
	let build_dir = out_root.join("build");
	let install_dir = out_root.join("install");
	let options_path = out_root.join("meson-options.txt");
	fs::create_dir_all(&out_root)
		.with_context(|| format!("failed to create {}", out_root.display()))?;

	let options = linux_pam_meson_options();
	let options_text = format!("{}\n", options.join("\n"));
	let existing_options = fs::read_to_string(&options_path).ok();
	let needs_reconfigure = existing_options.as_deref() != Some(options_text.as_str());
	let configured = build_dir.join("build.ninja").exists();

	if !configured {
		let mut setup_args = vec![
			"setup".to_string(),
			build_dir.display().to_string(),
			pam_src.display().to_string(),
		];
		setup_args.extend(options.clone());
		let setup_refs: Vec<&str> = setup_args.iter().map(String::as_str).collect();
		run_cmd(repo_root, "meson", &setup_refs)?;
		fs::write(&options_path, &options_text)
			.with_context(|| format!("failed to write {}", options_path.display()))?;
	} else if needs_reconfigure {
		let mut setup_args = vec![
			"setup".to_string(),
			"--reconfigure".to_string(),
			build_dir.display().to_string(),
			pam_src.display().to_string(),
		];
		setup_args.extend(options.clone());
		let setup_refs: Vec<&str> = setup_args.iter().map(String::as_str).collect();
		run_cmd(repo_root, "meson", &setup_refs)?;
		fs::write(&options_path, &options_text)
			.with_context(|| format!("failed to write {}", options_path.display()))?;
	}

	run_cmd(
		repo_root,
		"meson",
		&[
			"compile",
			"-C",
			build_dir
				.to_str()
				.ok_or_else(|| anyhow!("invalid linux-pam build dir"))?,
		],
	)?;

	if install_dir.exists() {
		fs::remove_dir_all(&install_dir)
			.with_context(|| format!("failed to clean {}", install_dir.display()))?;
	}
	fs::create_dir_all(&install_dir)
		.with_context(|| format!("failed to create {}", install_dir.display()))?;

	run_cmd(
		repo_root,
		"meson",
		&[
			"install",
			"-C",
			build_dir
				.to_str()
				.ok_or_else(|| anyhow!("invalid linux-pam build dir"))?,
			"--no-rebuild",
			"--destdir",
			install_dir
				.to_str()
				.ok_or_else(|| anyhow!("invalid linux-pam install dir"))?,
		],
	)?;

	let pam_lib = install_dir.join("usr/lib/x86_64-linux-gnu/libpam.so.0");
	if !pam_lib.exists() {
		bail!("linux-pam install did not produce {}", pam_lib.display());
	}

	Ok(())
}

fn linux_pam_meson_options() -> Vec<String> {
	vec![
		"--prefix=/usr".to_string(),
		"--sysconfdir=/etc".to_string(),
		"--libdir=lib/x86_64-linux-gnu".to_string(),
		"-Ddocs=disabled".to_string(),
		"-Di18n=disabled".to_string(),
		"-Daudit=disabled".to_string(),
		"-Dselinux=disabled".to_string(),
		"-Dlogind=disabled".to_string(),
		"-Delogind=disabled".to_string(),
		"-Deconf=disabled".to_string(),
		"-Dexamples=false".to_string(),
		"-Dxtests=false".to_string(),
		"-Dsecuredir=/usr/lib/x86_64-linux-gnu/security".to_string(),
	]
}

fn build_shadow(repo_root: &Path) -> Result<()> {
	let shadow_src = repo_root.join("src/system/auth/shadow");
	if !shadow_src.join("configure.ac").exists() {
		bail!(
			"shadow source not found in {}; run upstream import shadow first",
			shadow_src.display()
		);
	}

	if !shadow_src.join("configure").exists() {
		run_cmd(&shadow_src, "autoreconf", &["-v", "-f", "-i"])?;
	}

	let out_root = repo_root.join("out/build/shadow");
	let build_dir = out_root.join("build");
	let install_dir = out_root.join("install");
	let stamp = build_dir.join("config.stamp");
	fs::create_dir_all(&build_dir)
		.with_context(|| format!("failed to create {}", build_dir.display()))?;

	if !stamp.exists() {
		run_cmd(
			&build_dir,
			shadow_src
				.join("configure")
				.to_str()
				.ok_or_else(|| anyhow!("invalid shadow configure path"))?,
			&[
				"--prefix=/usr",
				"--sysconfdir=/etc",
				"--disable-nls",
				"--with-libpam",
				"--without-selinux",
			],
		)?;
		fs::write(&stamp, "configured\n")
			.with_context(|| format!("failed to write {}", stamp.display()))?;
	}

	run_cmd(&build_dir, "make", &["-j", "4"])?;

	if install_dir.exists() {
		fs::remove_dir_all(&install_dir)
			.with_context(|| format!("failed to clean {}", install_dir.display()))?;
	}
	fs::create_dir_all(&install_dir)
		.with_context(|| format!("failed to create {}", install_dir.display()))?;

	run_cmd(
		&build_dir,
		"make",
		&[
			"install",
			&format!(
				"DESTDIR={}",
				install_dir
					.to_str()
					.ok_or_else(|| anyhow!("invalid shadow install dir"))?
			),
		],
	)?;

	let passwd_bin = install_dir.join("usr/bin/passwd");
	if !passwd_bin.exists() {
		bail!("shadow install did not produce {}", passwd_bin.display());
	}

	Ok(())
}

fn build_sudo_rs(repo_root: &Path) -> Result<()> {
	let sudo_src = repo_root.join("src/system/auth/sudo-rs");
	if !sudo_src.join("Cargo.toml").exists() {
		bail!(
			"sudo-rs source not found in {}; run upstream import sudo-rs first",
			sudo_src.display()
		);
	}

	let pam_install = repo_root.join("out/build/linux-pam/install");
	let pam_lib = pam_install.join("usr/lib/x86_64-linux-gnu");
	if !pam_lib.join("libpam.so").exists() && !pam_lib.join("libpam.so.0").exists() {
		bail!(
			"linux-pam libraries missing at {}; run build pam first",
			pam_lib.display()
		);
	}
	let current_rustflags = std::env::var("RUSTFLAGS").unwrap_or_default();
	let rustflags = if current_rustflags.is_empty() {
		format!("-L native={}", pam_lib.display())
	} else {
		format!("-L native={} {current_rustflags}", pam_lib.display())
	};
	let current_library_path = std::env::var("LIBRARY_PATH").unwrap_or_default();
	let library_path = if current_library_path.is_empty() {
		pam_lib.display().to_string()
	} else {
		format!("{}:{current_library_path}", pam_lib.display())
	};
	let env_overrides = vec![
		("RUSTFLAGS", rustflags),
		("LIBRARY_PATH", library_path),
	];

	run_cmd_with_env_overrides(
		repo_root,
		"cargo",
		&[
			"build",
			"--release",
			"--manifest-path",
			"src/system/auth/sudo-rs/Cargo.toml",
			"--bin",
			"sudo",
			"--bin",
			"visudo",
		],
		&env_overrides,
	)?;

	let out_root = repo_root.join("out/build/sudo-rs");
	let install_dir = out_root.join("install");
	if install_dir.exists() {
		fs::remove_dir_all(&install_dir)
			.with_context(|| format!("failed to clean {}", install_dir.display()))?;
	}
	fs::create_dir_all(install_dir.join("usr/bin"))
		.with_context(|| format!("failed to create {}", install_dir.join("usr/bin").display()))?;

	for bin in ["sudo", "visudo"] {
		let src = repo_root.join(format!("src/system/auth/sudo-rs/target/release/{bin}"));
		if !src.exists() {
			bail!("sudo-rs build did not produce {}", src.display());
		}
		let dst = install_dir.join("usr/bin").join(bin);
		fs::copy(&src, &dst).with_context(|| format!("failed to copy {}", src.display()))?;
	}

	Ok(())
}

fn build_util_linux(repo_root: &Path) -> Result<()> {
	let util_linux_src = repo_root.join("src/userland/util-linux");
	if !util_linux_src.join("meson.build").exists() {
		bail!(
			"util-linux source not found in {}; run upstream import util-linux first",
			util_linux_src.display()
		);
	}

	let out_root = repo_root.join("out/build/util-linux");
	let build_dir = out_root.join("build");
	let install_dir = out_root.join("install");
	let options_path = out_root.join("meson-options.txt");
	let env_path = out_root.join("meson-env.txt");
	let pam_install = repo_root.join("out/build/linux-pam/install");
	let pam_pkgconfig = pam_install.join("usr/lib/x86_64-linux-gnu/pkgconfig");
	let pam_include = pam_install.join("usr/include");
	let pam_lib = pam_install.join("usr/lib/x86_64-linux-gnu");
	if !pam_pkgconfig.exists() {
		bail!(
			"linux-pam pkg-config directory missing at {}; run build pam first",
			pam_pkgconfig.display()
		);
	}

	let current_pkg_config = std::env::var("PKG_CONFIG_PATH").unwrap_or_default();
	let pkg_config_path = if current_pkg_config.is_empty() {
		pam_pkgconfig.display().to_string()
	} else {
		format!("{}:{current_pkg_config}", pam_pkgconfig.display())
	};
	let current_cflags = std::env::var("CFLAGS").unwrap_or_default();
	let cflags = if current_cflags.is_empty() {
		format!("-I{}", pam_include.display())
	} else {
		format!("-I{} {current_cflags}", pam_include.display())
	};
	let current_ldflags = std::env::var("LDFLAGS").unwrap_or_default();
	let ldflags = if current_ldflags.is_empty() {
		format!("-L{}", pam_lib.display())
	} else {
		format!("-L{} {current_ldflags}", pam_lib.display())
	};
	let env_overrides = vec![
		("PKG_CONFIG_PATH", pkg_config_path),
		("CFLAGS", cflags),
		("LDFLAGS", ldflags),
	];
	let env_text = format!(
		"PKG_CONFIG_PATH={}\nCFLAGS={}\nLDFLAGS={}\n",
		env_overrides[0].1, env_overrides[1].1, env_overrides[2].1
	);
	let existing_env = fs::read_to_string(&env_path).ok();
	fs::create_dir_all(&out_root)
		.with_context(|| format!("failed to create {}", out_root.display()))?;

	let options = util_linux_meson_options();
	let options_text = format!("{}\n", options.join("\n"));
	let existing_options = fs::read_to_string(&options_path).ok();
	let needs_reconfigure = existing_options.as_deref() != Some(options_text.as_str());
	let env_changed = existing_env.as_deref() != Some(env_text.as_str());
	let mut configured = build_dir.join("build.ninja").exists();

	if configured && env_changed {
		fs::remove_dir_all(&build_dir)
			.with_context(|| format!("failed to reset {}", build_dir.display()))?;
		configured = false;
	}

	if !configured {
		let mut setup_args = vec![
			"setup".to_string(),
			build_dir.display().to_string(),
			util_linux_src.display().to_string(),
		];
		setup_args.extend(options.clone());
		let setup_refs: Vec<&str> = setup_args.iter().map(String::as_str).collect();
		run_cmd_with_env_overrides(repo_root, "meson", &setup_refs, &env_overrides)?;
		fs::write(&options_path, &options_text)
			.with_context(|| format!("failed to write {}", options_path.display()))?;
		fs::write(&env_path, &env_text)
			.with_context(|| format!("failed to write {}", env_path.display()))?;
	} else if needs_reconfigure {
		let mut setup_args = vec![
			"setup".to_string(),
			"--reconfigure".to_string(),
			build_dir.display().to_string(),
			util_linux_src.display().to_string(),
		];
		setup_args.extend(options.clone());
		let setup_refs: Vec<&str> = setup_args.iter().map(String::as_str).collect();
		run_cmd_with_env_overrides(repo_root, "meson", &setup_refs, &env_overrides)?;
		fs::write(&options_path, &options_text)
			.with_context(|| format!("failed to write {}", options_path.display()))?;
		fs::write(&env_path, &env_text)
			.with_context(|| format!("failed to write {}", env_path.display()))?;
	}

	run_cmd_with_env_overrides(
		repo_root,
		"ninja",
		&[
			"-C",
			build_dir
				.to_str()
				.ok_or_else(|| anyhow!("invalid util-linux build dir"))?,
			"agetty",
			"login",
			"su",
		],
		&env_overrides,
	)?;

	if install_dir.exists() {
		fs::remove_dir_all(&install_dir)
			.with_context(|| format!("failed to clean {}", install_dir.display()))?;
	}
	fs::create_dir_all(&install_dir)
		.with_context(|| format!("failed to create {}", install_dir.display()))?;

	run_cmd_with_env_overrides(
		repo_root,
		"meson",
		&[
			"install",
			"-C",
			build_dir
				.to_str()
				.ok_or_else(|| anyhow!("invalid util-linux build dir"))?,
			"--no-rebuild",
			"--destdir",
			install_dir
				.to_str()
				.ok_or_else(|| anyhow!("invalid util-linux install dir"))?,
		],
		&env_overrides,
	)?;

	for path in [
		install_dir.join("usr/sbin/agetty"),
		install_dir.join("usr/bin/login"),
		install_dir.join("usr/bin/su"),
	] {
		if !path.exists() {
			bail!("util-linux install did not produce {}", path.display());
		}
	}

	Ok(())
}

fn util_linux_meson_options() -> Vec<String> {
	vec![
		"--prefix=/usr".to_string(),
		"--sbindir=/usr/sbin".to_string(),
		"--libdir=lib/x86_64-linux-gnu".to_string(),
		"--auto-features=disabled".to_string(),
		"-Dbuild-agetty=enabled".to_string(),
		"-Dbuild-login=enabled".to_string(),
		"-Dbuild-su=enabled".to_string(),
		"-Dsystemd=disabled".to_string(),
		"-Dnls=disabled".to_string(),
		"-Dbuild-bash-completion=disabled".to_string(),
		"-Dbuild-python=disabled".to_string(),
		"-Dbuild-pylibmount=disabled".to_string(),
		"-Dbuild-mount=disabled".to_string(),
	]
}

fn build_systemd(repo_root: &Path) -> Result<()> {
	let systemd_src = repo_root.join("src/system/systemd");
	if !systemd_src.join("meson.build").exists() {
		bail!(
			"systemd source not found in {}; run upstream import systemd first",
			systemd_src.display()
		);
	}

	let out_root = repo_root.join("out/build/systemd");
	let build_dir = out_root.join("build");
	let install_dir = out_root.join("install");
	let options_path = out_root.join("meson-options.txt");
	fs::create_dir_all(&out_root)
		.with_context(|| format!("failed to create {}", out_root.display()))?;

	let options = systemd_meson_options();
	let options_text = format!("{}\n", options.join("\n"));
	let existing_options = fs::read_to_string(&options_path).ok();
	let needs_reconfigure = existing_options.as_deref() != Some(options_text.as_str());
	let configured = build_dir.join("build.ninja").exists();

	if !configured {
		let mut setup_args = vec![
			"setup".to_string(),
			build_dir.display().to_string(),
			systemd_src.display().to_string(),
		];
		setup_args.extend(options.clone());
		let setup_refs: Vec<&str> = setup_args.iter().map(String::as_str).collect();
		run_cmd(repo_root, "meson", &setup_refs)?;
		fs::write(&options_path, &options_text)
			.with_context(|| format!("failed to write {}", options_path.display()))?;
	} else if needs_reconfigure {
		let mut setup_args = vec![
			"setup".to_string(),
			"--reconfigure".to_string(),
			build_dir.display().to_string(),
			systemd_src.display().to_string(),
		];
		setup_args.extend(options.clone());
		let setup_refs: Vec<&str> = setup_args.iter().map(String::as_str).collect();
		run_cmd(repo_root, "meson", &setup_refs)?;
		fs::write(&options_path, &options_text)
			.with_context(|| format!("failed to write {}", options_path.display()))?;
	}

	let ninja_args = vec!["-C", build_dir.to_str().ok_or_else(|| anyhow!("invalid build dir"))?];
	run_cmd(repo_root, "ninja", &ninja_args)?;

	if install_dir.exists() {
		fs::remove_dir_all(&install_dir)
			.with_context(|| format!("failed to clean {}", install_dir.display()))?;
	}
	fs::create_dir_all(&install_dir)
		.with_context(|| format!("failed to create {}", install_dir.display()))?;

	let install_args = vec![
		"install",
		"-C",
		build_dir.to_str().ok_or_else(|| anyhow!("invalid build dir"))?,
		"--no-rebuild",
		"--destdir",
		install_dir
			.to_str()
			.ok_or_else(|| anyhow!("invalid install dir"))?,
	];
	run_cmd(repo_root, "meson", &install_args)?;

	let pid1 = install_dir.join("usr/lib/systemd/systemd");
	if !pid1.exists() {
		bail!("systemd install did not produce {}", pid1.display());
	}

	Ok(())
}

fn systemd_meson_options() -> Vec<String> {
	vec![
		"--prefix=/usr".to_string(),
		"--sysconfdir=/etc".to_string(),
		"--localstatedir=/var".to_string(),
		"--libdir=lib/x86_64-linux-gnu".to_string(),
		"-Dmode=release".to_string(),
		"-Dtests=false".to_string(),
		"-Dman=disabled".to_string(),
		"-Dhtml=disabled".to_string(),
		"-Dtranslations=false".to_string(),
		"-Dnetworkd=false".to_string(),
		"-Dresolve=false".to_string(),
		"-Dtimesyncd=false".to_string(),
		"-Dhomed=disabled".to_string(),
		"-Dportabled=false".to_string(),
		"-Dnspawn=disabled".to_string(),
		"-Dbootloader=disabled".to_string(),
		"-Dfirstboot=false".to_string(),
		"-Drepart=disabled".to_string(),
		"-Doomd=false".to_string(),
		"-Duserdb=false".to_string(),
		"-Dremote=disabled".to_string(),
		"-Dsysupdate=disabled".to_string(),
		"-Dsysupdated=disabled".to_string(),
		"-Dsysinstall=false".to_string(),
		"-Dimportd=disabled".to_string(),
		"-Dvmspawn=disabled".to_string(),
		"-Dcoredump=false".to_string(),
		"-Dpstore=false".to_string(),
		"-Dmachined=false".to_string(),
		"-Dhostnamed=false".to_string(),
		"-Dlocaled=false".to_string(),
		"-Dtimedated=false".to_string(),
		"-Dnsresourced=false".to_string(),
		"-Ddefault-network=false".to_string(),
		"-Ddbus=disabled".to_string(),
		"-Dglib=disabled".to_string(),
		"-Dseccomp=disabled".to_string(),
		"-Dacl=disabled".to_string(),
		"-Daudit=disabled".to_string(),
		"-Dblkid=disabled".to_string(),
		"-Dkmod=disabled".to_string(),
		"-Dlibmount=enabled".to_string(),
		"-Dpam=disabled".to_string(),
		"-Dlibcryptsetup=disabled".to_string(),
		"-Dopenssl=disabled".to_string(),
		"-Dgnutls=disabled".to_string(),
		"-Dlibfido2=disabled".to_string(),
		"-Dtpm=false".to_string(),
		"-Dtpm2=disabled".to_string(),
		"-Dqrencode=disabled".to_string(),
		"-Dbpf-framework=disabled".to_string(),
		"-Dvmlinux-h=disabled".to_string(),
		"-Dkernel-install=false".to_string(),
		"-Danalyze=false".to_string(),
		"-Dcreate-log-dirs=false".to_string(),
		"-Djournal-storage-default=volatile".to_string(),
	]
}

fn build_rootfs(repo_root: &Path) -> Result<()> {
	let skeleton = repo_root.join("src/rootfs/skeleton");
	let out = repo_root.join("out/build/rootfs");

	if out.exists() {
		fs::remove_dir_all(&out).with_context(|| format!("failed to clean {}", out.display()))?;
	}
	copy_tree_excluding_dotgit(&skeleton, &out)?;
	ensure_merged_usr_layout(&out)?;
	set_mode(out.join("usr/libexec/mattos/brush-login"), 0o755)?;
	set_mode(out.join("usr/libexec/mattos/validate-shell-env"), 0o755)?;
	fs::create_dir_all(out.join("root")).context("failed to create /root in rootfs")?;
	set_mode(out.join("root"), 0o700)?;
	fs::create_dir_all(out.join("home")).context("failed to create /home in rootfs")?;
	fs::create_dir_all(out.join("run")).context("failed to create /run in rootfs")?;
	fs::create_dir_all(out.join("var/log")).context("failed to create /var/log in rootfs")?;
	fs::create_dir_all(out.join("var/tmp")).context("failed to create /var/tmp in rootfs")?;
	fs::create_dir_all(out.join("etc/systemd/system")).context("failed to create /etc/systemd/system")?;
	fs::create_dir_all(out.join("usr/libexec/mattos")).context("failed to create rescue init dir")?;
	fs::write(out.join("etc/machine-id"), "").context("failed to create /etc/machine-id")?;

	let pam_install = repo_root.join("out/build/linux-pam/install");
	let shadow_install = repo_root.join("out/build/shadow/install");
	let sudo_rs_install = repo_root.join("out/build/sudo-rs/install");
	let systemd_install = repo_root.join("out/build/systemd/install");
	let util_linux_install = repo_root.join("out/build/util-linux/install");
	let systemd_pid1 = systemd_install.join("usr/lib/systemd/systemd");
	let pam_lib = pam_install.join("usr/lib/x86_64-linux-gnu/libpam.so.0");
	let shadow_passwd = shadow_install.join("usr/bin/passwd");
	let sudo_bin = sudo_rs_install.join("usr/bin/sudo");
	if !systemd_pid1.exists() {
		bail!(
			"systemd install output missing at {}; run build systemd first",
			systemd_pid1.display()
		);
	}
	if !pam_lib.exists() {
		bail!(
			"linux-pam install output missing at {}; run build pam first",
			pam_lib.display()
		);
	}
	if !shadow_passwd.exists() {
		bail!(
			"shadow install output missing at {}; run build shadow first",
			shadow_passwd.display()
		);
	}
	if !sudo_bin.exists() {
		bail!(
			"sudo-rs install output missing at {}; run build sudo-rs first",
			sudo_bin.display()
		);
	}
	copy_tree_excluding_dotgit(&systemd_install, &out)?;
	copy_tree_excluding_dotgit(&pam_install, &out)?;
	copy_shared_object_and_deps("libmount.so.1", &out)?;
	copy_host_binary_and_deps("/usr/bin/mount", &out)?;
	copy_host_binary_and_deps("/usr/sbin/ldconfig", &out)?;
	for rel in ["usr/sbin/agetty", "usr/bin/login", "usr/bin/su"] {
		let src = util_linux_install.join(rel);
		if !src.exists() {
			bail!(
				"util-linux install output missing at {}; run build util-linux first",
				src.display()
			);
		}
		let dst = out.join(rel);
		if let Some(parent) = dst.parent() {
			fs::create_dir_all(parent)
				.with_context(|| format!("failed to create {}", parent.display()))?;
		}
		fs::copy(&src, &dst).with_context(|| format!("failed to copy {}", src.display()))?;
		copy_runtime_dependencies(&dst, &out)?;
	}

	for rel in [
		"usr/bin/passwd",
		"usr/sbin/useradd",
		"usr/sbin/usermod",
		"usr/sbin/userdel",
		"usr/sbin/groupadd",
		"usr/sbin/groupmod",
		"usr/sbin/groupdel",
		"usr/sbin/chpasswd",
		"usr/bin/chage",
		"usr/bin/newgrp",
		"usr/bin/groups",
	] {
		copy_built_binary_and_runtime(&shadow_install.join(rel), &out.join(rel), &out)?;
	}

	for rel in ["usr/bin/sudo", "usr/bin/visudo"] {
		copy_built_binary_and_runtime(&sudo_rs_install.join(rel), &out.join(rel), &out)?;
	}

	verify_required_pam_modules(&out)?;
	copy_auth_configuration(repo_root, &out)?;
	apply_live_profile(repo_root, &out)?;
	validate_account_database(&out)?;
	enforce_auth_file_modes(&out)?;
	install_mattos_system_units(repo_root, &out)?;

	let init_bin = repo_root.join("target/release/mattos-init");
	if !init_bin.exists() {
		bail!(
			"init binary missing at {}; run build init first",
			init_bin.display()
		);
	}

	let rescue_init = out.join("usr/libexec/mattos/rescue-init");
	fs::copy(&init_bin, &rescue_init).with_context(|| {
		format!(
			"failed to copy rescue init binary from {} into rootfs",
			init_bin.display()
		)
	})?;
	copy_runtime_dependencies(&rescue_init, &out)?;
	let mut inventory = UserlandInventory::default();
	inventory.add_implemented(UTIL_LINUX_PROVIDER, "agetty");
	inventory.add_implemented(UTIL_LINUX_PROVIDER, "login");
	inventory.add_implemented(UTIL_LINUX_PROVIDER, "su");
	inventory.add_compiled(UTIL_LINUX_PROVIDER, "agetty");
	inventory.add_compiled(UTIL_LINUX_PROVIDER, "login");
	inventory.add_compiled(UTIL_LINUX_PROVIDER, "su");
	inventory.add_installed(UTIL_LINUX_PROVIDER, "agetty");
	inventory.add_installed(UTIL_LINUX_PROVIDER, "login");
	inventory.add_installed(UTIL_LINUX_PROVIDER, "su");

	for module in [
		"libpam",
		"pam_unix",
		"pam_env",
		"pam_nologin",
		"pam_rootok",
		"pam_permit",
		"pam_deny",
		"pam_shells",
		"pam_securetty",
	] {
		inventory.add_implemented(LINUX_PAM_PROVIDER, module);
		inventory.add_compiled(LINUX_PAM_PROVIDER, module);
		inventory.add_installed(LINUX_PAM_PROVIDER, module);
	}

	for cmd in [
		"passwd",
		"useradd",
		"usermod",
		"userdel",
		"groupadd",
		"groupmod",
		"groupdel",
		"chpasswd",
		"chage",
		"newgrp",
		"groups",
	] {
		inventory.add_implemented(SHADOW_PROVIDER, cmd);
		inventory.add_compiled(SHADOW_PROVIDER, cmd);
		inventory.add_installed(SHADOW_PROVIDER, cmd);
	}
	inventory.add_implemented(SUDO_RS_PROVIDER, "sudo");
	inventory.add_compiled(SUDO_RS_PROVIDER, "sudo");
	inventory.add_installed(SUDO_RS_PROVIDER, "sudo");

	let brush_candidates = [
		repo_root.join("src/userland/brush/target/release/brush"),
		repo_root.join("src/userland/brush/target/release/brush"),
	];
	let brush_bin = brush_candidates.iter().find(|p| p.exists()).cloned();

	if let Some(brush_bin) = brush_bin {
		let dst = out.join("usr/bin/brush");
		fs::copy(&brush_bin, &dst).context("failed to copy brush binary")?;
		copy_runtime_dependencies(&dst, &out)?;
		inventory.add_implemented("brush", "brush");
		inventory.add_compiled("brush", "brush");
		inventory.add_installed("brush", "brush");
	} else {
		bail!("brush binary not found; run build brush first")
	}

	let coreutils_multicall = resolve_coreutils_multicall(repo_root)?;
	let coreutils_dst = out.join("usr/bin/coreutils");
	fs::copy(&coreutils_multicall, &coreutils_dst).with_context(|| {
		format!(
			"failed to copy coreutils multicall binary from {}",
			coreutils_multicall.display()
		)
	})?;
	copy_runtime_dependencies(&coreutils_dst, &out)?;

	let coreutils_applets = list_coreutils_applets(&coreutils_multicall)?;
	for applet in &coreutils_applets {
		inventory.add_implemented(COREUTILS_PROVIDER, applet);
		inventory.add_compiled(COREUTILS_PROVIDER, applet);
	}
	create_coreutils_symlinks(&out, &coreutils_applets)?;
	for applet in &coreutils_applets {
		inventory.add_installed(COREUTILS_PROVIDER, applet);
	}

	for spec in USERLAND_BINARY_INSTALLS {
		install_userland_binary(repo_root, &out, spec)?;
		inventory.add_implemented(spec.provider, spec.command_name);
		inventory.add_compiled(spec.provider, spec.command_name);
		inventory.add_installed(spec.provider, spec.command_name);
	}

	create_command_aliases(&out, "diffutils", DIFFUTILS_AVAILABLE_ALIASES)?;
	for alias in DIFFUTILS_AVAILABLE_ALIASES {
		inventory.add_implemented(DIFFUTILS_PROVIDER, alias);
		inventory.add_installed(DIFFUTILS_PROVIDER, alias);
	}
	for expected in DIFFUTILS_EXPECTED_COMMANDS {
		if !DIFFUTILS_AVAILABLE_ALIASES.contains(expected) {
			inventory.add_failed(DIFFUTILS_PROVIDER, expected, "not implemented upstream");
		}
	}

	let mut provider_commands = BTreeMap::<&str, Vec<String>>::new();
	provider_commands.insert(COREUTILS_PROVIDER, coreutils_applets.clone());
	for spec in USERLAND_BINARY_INSTALLS {
		provider_commands
			.entry(spec.provider)
			.or_default()
			.push(spec.command_name.to_string());
	}
	provider_commands
		.entry(DIFFUTILS_PROVIDER)
		.or_default()
		.extend(DIFFUTILS_AVAILABLE_ALIASES.iter().map(|s| s.to_string()));
	validate_no_duplicate_commands(&provider_commands)?;

	for expected in [
		"grep",
		"sed",
		"find",
		"xargs",
		"diff",
		"cmp",
		"login",
		"su",
		"passwd",
		"sudo",
		"useradd",
		"usermod",
		"userdel",
		"groupadd",
		"groupmod",
		"groupdel",
		"chpasswd",
	] {
		let path = out.join("usr/bin").join(expected);
		let alt = out.join("usr/sbin").join(expected);
		if !path_entry_exists(&path) && !path_entry_exists(&alt) {
			bail!(
				"required command {} missing from rootfs at {}",
				expected,
				path.display()
			)
		}
	}

	let sh_link = out.join("usr/bin/sh");
	if sh_link.exists() {
		fs::remove_file(&sh_link)
			.with_context(|| format!("failed to remove existing {}", sh_link.display()))?;
	}
	#[cfg(unix)]
	std::os::unix::fs::symlink("/bin/brush", &sh_link)
		.with_context(|| format!("failed to create {}", sh_link.display()))?;
	inventory.add_installed("brush", "sh");
	inventory.add_excluded(DIFFUTILS_PROVIDER, "diff3");
	inventory.add_excluded(DIFFUTILS_PROVIDER, "sdiff");
	write_userland_inventory(&out, &inventory)?;

	copy_systemd_runtime_dependencies(&out)?;

	Ok(())
}

#[cfg(unix)]
fn ensure_merged_usr_layout(rootfs: &Path) -> Result<()> {
	use std::os::unix::fs::symlink;

	fs::create_dir_all(rootfs.join("usr/bin")).context("failed to create /usr/bin")?;
	fs::create_dir_all(rootfs.join("usr/sbin")).context("failed to create /usr/sbin")?;
	fs::create_dir_all(rootfs.join("usr/lib")).context("failed to create /usr/lib")?;
	fs::create_dir_all(rootfs.join("usr/lib64")).context("failed to create /usr/lib64")?;

	for (name, target) in [
		("bin", "usr/bin"),
		("sbin", "usr/sbin"),
		("lib", "usr/lib"),
		("lib64", "usr/lib64"),
	] {
		let path = rootfs.join(name);
		if path.exists() {
			let meta = fs::symlink_metadata(&path)
				.with_context(|| format!("failed to stat {}", path.display()))?;
			if meta.file_type().is_symlink() {
				fs::remove_file(&path)
					.with_context(|| format!("failed to remove symlink {}", path.display()))?;
			} else if meta.is_dir() {
				fs::remove_dir_all(&path)
					.with_context(|| format!("failed to remove directory {}", path.display()))?;
			} else {
				fs::remove_file(&path)
					.with_context(|| format!("failed to remove file {}", path.display()))?;
			}
		}
		symlink(target, &path)
			.with_context(|| format!("failed to create symlink {} -> {}", path.display(), target))?;
	}

	Ok(())
}

#[cfg(not(unix))]
fn ensure_merged_usr_layout(_rootfs: &Path) -> Result<()> {
	bail!("merged /usr rootfs layout requires Unix symlink support")
}

fn install_mattos_system_units(repo_root: &Path, rootfs: &Path) -> Result<()> {
	let units_src = repo_root.join("src/system/units");
	if !units_src.exists() {
		bail!(
			"MattOS systemd units missing at {}; expected MattOS-owned units",
			units_src.display()
		);
	}
	let units_dst = rootfs.join("usr/lib/systemd/system");
	fs::create_dir_all(&units_dst)
		.with_context(|| format!("failed to create {}", units_dst.display()))?;
	copy_tree_excluding_dotgit(&units_src, &units_dst)?;

	let default_target = rootfs.join("etc/systemd/system/default.target");
	if default_target.exists() {
		fs::remove_file(&default_target)
			.with_context(|| format!("failed to remove {}", default_target.display()))?;
	}
	#[cfg(unix)]
	std::os::unix::fs::symlink("/usr/lib/systemd/system/mattos.target", &default_target)
		.with_context(|| format!("failed to create {}", default_target.display()))?;

	let getty_wants = rootfs.join("etc/systemd/system/getty.target.wants");
	fs::create_dir_all(&getty_wants)
		.with_context(|| format!("failed to create {}", getty_wants.display()))?;
	let tty1_getty = getty_wants.join("getty@tty1.service");
	if tty1_getty.exists() {
		fs::remove_file(&tty1_getty)
			.with_context(|| format!("failed to remove {}", tty1_getty.display()))?;
	}
	#[cfg(unix)]
	std::os::unix::fs::symlink("/usr/lib/systemd/system/getty@.service", &tty1_getty)
		.with_context(|| format!("failed to create {}", tty1_getty.display()))?;

	for masked in [
		"systemd-logind.service",
		"systemd-logind-varlink.socket",
		"ldconfig.service",
		"mattos-shell.service",
	] {
		let mask = rootfs.join("etc/systemd/system").join(masked);
		if mask.exists() {
			fs::remove_file(&mask)
				.with_context(|| format!("failed to remove {}", mask.display()))?;
		}
		#[cfg(unix)]
		std::os::unix::fs::symlink("/dev/null", &mask)
			.with_context(|| format!("failed to create {}", mask.display()))?;
	}

	Ok(())
}

fn copy_built_binary_and_runtime(src: &Path, dst: &Path, rootfs: &Path) -> Result<()> {
	if !src.exists() {
		bail!("required binary missing at {}", src.display());
	}
	if let Some(parent) = dst.parent() {
		fs::create_dir_all(parent)
			.with_context(|| format!("failed to create {}", parent.display()))?;
	}
	fs::copy(src, dst).with_context(|| format!("failed to copy {}", src.display()))?;
	copy_runtime_dependencies(dst, rootfs)
}

fn copy_auth_configuration(repo_root: &Path, rootfs: &Path) -> Result<()> {
	let auth_src = repo_root.join("src/system/auth/config");
	if !auth_src.exists() {
		bail!(
			"MattOS auth config missing at {}; expected local auth policy files",
			auth_src.display()
		);
	}

	let etc_dst = rootfs.join("etc");
	fs::create_dir_all(&etc_dst).with_context(|| format!("failed to create {}", etc_dst.display()))?;

	for (src_rel, dst_rel) in [
		("pam.d", "pam.d"),
		("sudoers.d", "sudoers.d"),
		("default", "default"),
	] {
		copy_tree_excluding_dotgit(&auth_src.join(src_rel), &etc_dst.join(dst_rel))?;
	}

	for (src_rel, dst_rel) in [
		("login.defs", "login.defs"),
		("sudoers", "sudoers"),
	] {
		fs::copy(auth_src.join(src_rel), etc_dst.join(dst_rel)).with_context(|| {
			format!(
				"failed to copy auth config {} to {}",
				auth_src.join(src_rel).display(),
				etc_dst.join(dst_rel).display()
			)
		})?;
	}

	Ok(())
}

fn apply_live_profile(repo_root: &Path, rootfs: &Path) -> Result<()> {
	let live_src = repo_root.join("src/system/profiles/live");
	if !live_src.exists() {
		bail!(
			"MattOS live profile missing at {}; expected live profile overlay",
			live_src.display()
		);
	}
	copy_tree_excluding_dotgit(&live_src, rootfs)?;

	let notice_script = rootfs.join("etc/profile.d/10-mattos-live-notice.sh");
	if notice_script.exists() {
		set_mode(notice_script, 0o755)?;
	}

	Ok(())
}

fn verify_required_pam_modules(rootfs: &Path) -> Result<()> {
	let security_dirs = [
		rootfs.join("usr/lib/x86_64-linux-gnu/security"),
		rootfs.join("usr/lib/security"),
	];
	let required = [
		"pam_unix.so",
		"pam_env.so",
		"pam_nologin.so",
		"pam_rootok.so",
		"pam_permit.so",
		"pam_deny.so",
		"pam_shells.so",
		"pam_securetty.so",
	];

	for module in required {
		let mut found = false;
		for dir in &security_dirs {
			if dir.join(module).exists() {
				found = true;
				break;
			}
		}
		if !found {
			bail!("required PAM module {} missing from rootfs security dirs", module);
		}
	}

	Ok(())
}

fn enforce_auth_file_modes(rootfs: &Path) -> Result<()> {
	for (rel, mode) in [
		("etc/shadow", 0o600),
		("etc/gshadow", 0o600),
		("etc/passwd", 0o644),
		("etc/group", 0o644),
		("etc/sudoers", 0o440),
		("usr/bin/login", 0o4755),
		("usr/bin/su", 0o4755),
		("usr/bin/passwd", 0o4755),
		("usr/bin/sudo", 0o4755),
	] {
		let path = rootfs.join(rel);
		if !path.exists() {
			bail!("expected auth file missing at {}", path.display());
		}
		set_mode(path, mode)?;
	}

	let sudoers_dir = rootfs.join("etc/sudoers.d");
	if !sudoers_dir.exists() {
		bail!("expected sudoers include dir missing at {}", sudoers_dir.display());
	}
	set_mode(sudoers_dir, 0o750)?;

	for rel in ["etc/sudoers.d/00-mattos-live", "etc/sudoers.d/README"] {
		let path = rootfs.join(rel);
		if path.exists() {
			set_mode(path, 0o440)?;
		}
	}

	let root_home = rootfs.join("root");
	if root_home.exists() {
		set_mode(root_home, 0o700)?;
	}
	let live_home = rootfs.join("home/mattos");
	if live_home.exists() {
		set_mode(live_home, 0o750)?;
	}

	Ok(())
}

fn validate_account_database(rootfs: &Path) -> Result<()> {
	let passwd_path = rootfs.join("etc/passwd");
	let group_path = rootfs.join("etc/group");
	let shadow_path = rootfs.join("etc/shadow");
	let gshadow_path = rootfs.join("etc/gshadow");

	for path in [&passwd_path, &group_path, &shadow_path, &gshadow_path] {
		if !path.exists() {
			bail!("required account database file missing at {}", path.display());
		}
	}

	let passwd_body = fs::read_to_string(&passwd_path)
		.with_context(|| format!("failed to read {}", passwd_path.display()))?;
	let group_body = fs::read_to_string(&group_path)
		.with_context(|| format!("failed to read {}", group_path.display()))?;

	if passwd_body.contains("matt-alienware") || passwd_body.contains("matt:") {
		bail!("passwd file appears to contain host developer username leakage")
	}

	let mut seen_uids = BTreeSet::<u32>::new();
	let mut seen_gids = BTreeSet::<u32>::new();
	let mut saw_root = false;
	let mut saw_live = false;

	for line in passwd_body.lines() {
		if line.trim().is_empty() {
			continue;
		}
		let parts: Vec<&str> = line.split(':').collect();
		if parts.len() != 7 {
			bail!("invalid passwd entry format: {line}");
		}
		let user = parts[0];
		let uid = parts[2]
			.parse::<u32>()
			.with_context(|| format!("invalid uid in passwd entry: {line}"))?;
		let gid = parts[3]
			.parse::<u32>()
			.with_context(|| format!("invalid gid in passwd entry: {line}"))?;

		if !seen_uids.insert(uid) {
			bail!("duplicate uid detected in passwd: {uid}")
		}

		if user == "root" {
			saw_root = true;
			if uid != 0 || gid != 0 || parts[5] != "/root" || parts[6] != "/bin/brush" {
				bail!("root account entry does not match expected MattOS policy")
			}
		}

		if user == "mattos" {
			saw_live = true;
			if uid != 1000 || gid != 1000 || parts[5] != "/home/mattos" || parts[6] != "/bin/brush" {
				bail!("live user mattos entry does not match expected MattOS policy")
			}
		}
	}

	if !saw_root {
		bail!("root account missing from passwd")
	}
	if !saw_live {
		bail!("live user mattos missing from passwd")
	}

	let mut saw_sudo_group = false;
	for line in group_body.lines() {
		if line.trim().is_empty() {
			continue;
		}
		let parts: Vec<&str> = line.split(':').collect();
		if parts.len() != 4 {
			bail!("invalid group entry format: {line}");
		}
		let name = parts[0];
		let gid = parts[2]
			.parse::<u32>()
			.with_context(|| format!("invalid gid in group entry: {line}"))?;
		if !seen_gids.insert(gid) {
			bail!("duplicate gid detected in group: {gid}")
		}
		if name == "sudo" {
			saw_sudo_group = true;
			if !parts[3].split(',').any(|m| m == "mattos") {
				bail!("sudo group exists but mattos is not a member")
			}
		}
	}

	if !saw_sudo_group {
		bail!("sudo administrative group missing from group database")
	}

	Ok(())
}

fn set_mode(path: PathBuf, mode: u32) -> Result<()> {
	#[cfg(unix)]
	{
		let perms = std::os::unix::fs::PermissionsExt::from_mode(mode);
		fs::set_permissions(&path, perms)
			.with_context(|| format!("failed to set mode {:o} on {}", mode, path.display()))?;
	}
	#[cfg(not(unix))]
	{
		let _ = (path, mode);
	}
	Ok(())
}

fn copy_systemd_runtime_dependencies(rootfs: &Path) -> Result<()> {
	let mut binaries = Vec::new();
	for rel in [
		"usr/lib/systemd/systemd",
		"usr/lib/systemd/systemd-journald",
		"usr/lib/systemd/systemd-udevd",
		"usr/bin/systemctl",
		"usr/bin/journalctl",
	] {
		let p = rootfs.join(rel);
		if p.exists() {
			binaries.push(p);
		}
	}

	for bin in binaries {
		copy_runtime_dependencies(&bin, rootfs)?;
	}
	Ok(())
}

#[cfg(unix)]
fn create_coreutils_symlinks(rootfs: &Path, applets: &[String]) -> Result<()> {
	use std::os::unix::fs::symlink;

	let bin = rootfs.join("bin");
	let usr_bin = rootfs.join("usr/bin");
	fs::create_dir_all(&usr_bin)
		.with_context(|| format!("failed to create {}", usr_bin.display()))?;
	for applet in applets {
		let link = bin.join(applet);
		if path_entry_exists(&link) {
			fs::remove_file(&link)
				.with_context(|| format!("failed to remove existing symlink {}", link.display()))?;
		}
		symlink("/bin/coreutils", &link)
			.with_context(|| format!("failed to create symlink {}", link.display()))?;

		let usr_link = usr_bin.join(applet);
		if path_entry_exists(&usr_link) {
			fs::remove_file(&usr_link)
				.with_context(|| format!("failed to remove existing symlink {}", usr_link.display()))?;
		}
		symlink("/bin/coreutils", &usr_link)
			.with_context(|| format!("failed to create symlink {}", usr_link.display()))?;
	}
	Ok(())
}

#[cfg(not(unix))]
fn create_coreutils_symlinks(_rootfs: &Path, _applets: &[String]) -> Result<()> {
	println!("warning: coreutils symlink generation skipped on non-Unix host");
	Ok(())
}

fn resolve_coreutils_multicall(repo_root: &Path) -> Result<PathBuf> {
	let candidates = [
		repo_root.join("src/userland/coreutils/target/release/coreutils"),
		repo_root.join("src/userland/coreutils/target/release/uutils"),
	];
	candidates
		.iter()
		.find(|p| p.exists())
		.cloned()
		.ok_or_else(|| anyhow!("coreutils multicall binary not found; run build coreutils first"))
}

fn list_coreutils_applets(coreutils_multicall: &Path) -> Result<Vec<String>> {
	let output = Command::new(coreutils_multicall)
		.arg("--list")
		.output()
		.with_context(|| format!("failed to run {} --list", coreutils_multicall.display()))?;
	if !output.status.success() {
		bail!("coreutils --list failed with status {}", output.status)
	}

	let raw = String::from_utf8(output.stdout).context("coreutils --list output was not UTF-8")?;
	let mut applets: Vec<String> = raw
		.lines()
		.map(str::trim)
		.filter(|line| !line.is_empty())
		.filter(|line| !line.starts_with('<') && *line != "uutils")
		.map(ToOwned::to_owned)
		.collect();
	applets.sort();
	applets.dedup();
	if applets.is_empty() {
		bail!("coreutils --list returned no applets")
	}
	Ok(applets)
}

fn install_userland_binary(repo_root: &Path, rootfs: &Path, spec: &BinaryInstallSpec) -> Result<()> {
	let source = repo_root.join(spec.source_rel);
	if !source.exists() {
		bail!(
			"{} binary missing at {}; run the matching build stage first",
			spec.command_name,
			source.display()
		)
	}

	let dst = rootfs.join("usr/bin").join(spec.install_name);
	if let Some(parent) = dst.parent() {
		fs::create_dir_all(parent)
			.with_context(|| format!("failed to create {}", parent.display()))?;
	}
	fs::copy(&source, &dst)
		.with_context(|| format!("failed to copy {} into rootfs", source.display()))?;
	copy_runtime_dependencies(&dst, rootfs)?;
	Ok(())
}

#[cfg(unix)]
fn create_command_aliases(rootfs: &Path, target_binary: &str, aliases: &[&str]) -> Result<()> {
	use std::os::unix::fs::symlink;

	let usr_bin = rootfs.join("usr/bin");
	for alias in aliases {
		let link = usr_bin.join(alias);
		if path_entry_exists(&link) {
			fs::remove_file(&link)
				.with_context(|| format!("failed to remove existing alias {}", link.display()))?;
		}
		symlink(format!("/bin/{target_binary}"), &link)
			.with_context(|| format!("failed to create alias {}", link.display()))?;
	}
	Ok(())
}

#[cfg(not(unix))]
fn create_command_aliases(_rootfs: &Path, _target_binary: &str, _aliases: &[&str]) -> Result<()> {
	bail!("command alias generation requires Unix symlink support")
}

fn validate_no_duplicate_commands(provider_commands: &BTreeMap<&str, Vec<String>>) -> Result<()> {
	let mut owners = BTreeMap::<String, Vec<&str>>::new();
	for (provider, commands) in provider_commands {
		for command in commands {
			owners.entry(command.clone()).or_default().push(provider);
		}
	}

	let duplicates: Vec<String> = owners
		.iter()
		.filter_map(|(cmd, providers)| {
			if providers.len() > 1 {
				Some(format!("{} [{}]", cmd, providers.join(", ")))
			} else {
				None
			}
		})
		.collect();

	if !duplicates.is_empty() {
		bail!(
			"duplicate command ownership detected: {}",
			duplicates.join("; ")
		)
	}

	Ok(())
}

fn path_entry_exists(path: &Path) -> bool {
	fs::symlink_metadata(path).is_ok()
}

fn write_userland_inventory(rootfs: &Path, inventory: &UserlandInventory) -> Result<()> {
	let path = rootfs.join(USERLAND_INVENTORY_PATH);
	if let Some(parent) = path.parent() {
		fs::create_dir_all(parent)
			.with_context(|| format!("failed to create {}", parent.display()))?;
	}

	let mut lines = Vec::new();
	lines.push("# MattOS userland command inventory".to_string());
	lines.push("# format: provider:command".to_string());
	lines.push(String::new());
	lines.push("[implemented_upstream]".to_string());
	for entry in &inventory.implemented_upstream {
		lines.push(entry.clone());
	}
	lines.push(String::new());
	lines.push("[compiled]".to_string());
	for entry in &inventory.compiled {
		lines.push(entry.clone());
	}
	lines.push(String::new());
	lines.push("[installed]".to_string());
	for entry in &inventory.installed {
		lines.push(entry.clone());
	}
	lines.push(String::new());
	lines.push("[intentionally_excluded]".to_string());
	for entry in &inventory.intentionally_excluded {
		lines.push(entry.clone());
	}
	lines.push(String::new());
	lines.push("[failed_compatibility]".to_string());
	for entry in &inventory.failed_compatibility {
		lines.push(entry.clone());
	}

	fs::write(&path, lines.join("\n") + "\n")
		.with_context(|| format!("failed to write {}", path.display()))
}

fn build_initramfs(repo_root: &Path) -> Result<()> {
	let rootfs = repo_root.join("out/build/rootfs");
	if !rootfs.exists() {
		bail!("rootfs not found; run build rootfs first");
	}

	let out_build = repo_root.join("out/build");
	fs::create_dir_all(&out_build).context("failed to create out/build directory")?;

	run_cmd(
		&rootfs,
		"bash",
		&[
			"-lc",
			"find . -print0 | cpio --null -ov --owner=0:0 --format=newc | gzip -9 > ../initramfs.cpio.gz",
		],
	)
}

fn build_iso(repo_root: &Path) -> Result<()> {
	let grub_src = validate_grub_config_source(repo_root)?;

	let kernel = repo_root.join("src/kernel/linux/arch/x86/boot/bzImage");
	if !kernel.exists() {
		bail!(
			"kernel image missing at {}; build kernel first",
			kernel.display()
		);
	}

	let initramfs = repo_root.join("out/build/initramfs.cpio.gz");
	if !initramfs.exists() {
		bail!("initramfs missing at {}; run build initramfs", initramfs.display());
	}

	let iso_root = repo_root.join("out/build/iso");
	if iso_root.exists() {
		fs::remove_dir_all(&iso_root)
			.with_context(|| format!("failed to clean {}", iso_root.display()))?;
	}
	let grub_dir = iso_root.join("boot/grub");
	fs::create_dir_all(&grub_dir).context("failed to create ISO directory layout")?;

	fs::copy(&kernel, iso_root.join("boot/vmlinuz")).context("failed to stage kernel into ISO tree")?;
	fs::copy(&initramfs, iso_root.join("boot/initramfs.cpio.gz"))
		.context("failed to stage initramfs into ISO tree")?;
	let staged_grub_cfg = grub_dir.join("grub.cfg");
	fs::copy(&grub_src, &staged_grub_cfg).context("failed to copy grub config")?;
	validate_staged_grub_config(&staged_grub_cfg)?;

	let src_grub_text = fs::read_to_string(&grub_src)
		.with_context(|| format!("failed to read {}", grub_src.display()))?;
	let staged_grub_text = fs::read_to_string(&staged_grub_cfg)
		.with_context(|| format!("failed to read {}", staged_grub_cfg.display()))?;
	if src_grub_text != staged_grub_text {
		bail!(
			"staged GRUB config at {} differs from authoritative source {}",
			staged_grub_cfg.display(),
			grub_src.display()
		);
	}

	let out_images = repo_root.join("out/images");
	fs::create_dir_all(&out_images).context("failed to create out/images")?;
	run_cmd(
		repo_root,
		"grub-mkrescue",
		&[
			"-o",
			"out/images/mattos-x86_64.iso",
			"out/build/iso",
		],
	)
}

fn validate_grub_config_source(repo_root: &Path) -> Result<PathBuf> {
	let authoritative = repo_root.join(AUTHORITATIVE_GRUB_CFG);
	if !authoritative.exists() {
		bail!(
			"authoritative GRUB config missing at {}; expected single source at {}",
			authoritative.display(),
			AUTHORITATIVE_GRUB_CFG
		);
	}

	for obsolete in OBSOLETE_GRUB_CFG_PATHS {
		let obsolete_path = repo_root.join(obsolete);
		if obsolete_path.exists() {
			bail!(
				"obsolete GRUB config path detected at {}; remove stale duplicate and keep only {}",
				obsolete_path.display(),
				AUTHORITATIVE_GRUB_CFG
			);
		}
	}

	Ok(authoritative)
}

fn validate_staged_grub_config(path: &Path) -> Result<()> {
	let content = fs::read_to_string(path)
		.with_context(|| format!("failed to read staged grub config {}", path.display()))?;

	for needle in [
		GRUB_SYSTEMD_ENTRY,
		GRUB_RESCUE_ENTRY,
		GRUB_SYSTEMD_RDINIT,
		GRUB_RESCUE_RDINIT,
	] {
		if !content.contains(needle) {
			bail!(
				"staged GRUB config {} is missing required marker: {}",
				path.display(),
				needle
			);
		}
	}

	Ok(())
}

fn run_qemu(repo_root: &Path) -> Result<()> {
	let iso = repo_root.join("out/images/mattos-x86_64.iso");
	if !iso.exists() {
		bail!("ISO missing at {}; run build iso first", iso.display());
	}
	let logs = repo_root.join("out/logs");
	fs::create_dir_all(&logs).context("failed to create out/logs")?;
	let log_path = logs.join("qemu-boot.log");
	let serial_log_path = logs.join("qemu-serial.log");
	let serial_arg = format!(
		"file:{}",
		serial_log_path
			.to_str()
			.ok_or_else(|| anyhow!("invalid qemu serial log path"))?
	);

	run_cmd(
		repo_root,
		"qemu-system-x86_64",
		&[
			"-m",
			"1024",
			"-cdrom",
			iso.to_str().ok_or_else(|| anyhow!("invalid ISO path"))?,
			"-boot",
			"d",
			"-serial",
			serial_arg.as_str(),
			"-D",
			log_path
				.to_str()
				.ok_or_else(|| anyhow!("invalid qemu log path"))?,
		],
	)
}

fn copy_runtime_dependencies(binary: &Path, rootfs: &Path) -> Result<()> {
	let binary_str = binary
		.to_str()
		.ok_or_else(|| anyhow!("invalid binary path {}", binary.display()))?;
	let output = run_cmd_output(Path::new("/"), "ldd", &[binary_str])?;
	if !output.status.success() {
		return Ok(());
	}
	let text = String::from_utf8(output.stdout).context("ldd output was not UTF-8")?;

	for token in text.split_whitespace() {
		if !token.starts_with('/') {
			continue;
		}
		let src = Path::new(token);
		if !src.exists() {
			continue;
		}
		let rel = src.strip_prefix("/").unwrap_or(src);
		let dst = rootfs.join(rel);
		if let Some(parent) = dst.parent() {
			fs::create_dir_all(parent)
				.with_context(|| format!("failed to create {}", parent.display()))?;
		}
		fs::copy(src, &dst)
			.with_context(|| format!("failed to copy runtime dependency {}", src.display()))?;
	}

	Ok(())
}

fn copy_host_binary_and_deps(path: &str, rootfs: &Path) -> Result<()> {
	let src = Path::new(path);
	if !src.exists() {
		return Ok(());
	}

	let rel = src.strip_prefix("/").unwrap_or(src);
	let dst = rootfs.join(rel);
	if let Some(parent) = dst.parent() {
		fs::create_dir_all(parent)
			.with_context(|| format!("failed to create {}", parent.display()))?;
	}
	fs::copy(src, &dst)
		.with_context(|| format!("failed to copy host binary {}", src.display()))?;
	copy_runtime_dependencies(src, rootfs)?;
	Ok(())
}

fn copy_shared_object_and_deps(soname: &str, rootfs: &Path) -> Result<()> {
	let src = resolve_shared_object_path(soname)?;
	let rel = src.strip_prefix("/").unwrap_or(src.as_path());
	let dst = rootfs.join(rel);
	if let Some(parent) = dst.parent() {
		fs::create_dir_all(parent)
			.with_context(|| format!("failed to create {}", parent.display()))?;
	}
	fs::copy(&src, &dst)
		.with_context(|| format!("failed to copy runtime dependency {}", src.display()))?;
	copy_runtime_dependencies(&src, rootfs)?;
	Ok(())
}

fn resolve_shared_object_path(soname: &str) -> Result<PathBuf> {
	let output = run_cmd_output(Path::new("/"), "ldconfig", &["-p"])?;
	if output.status.success() {
		let text = String::from_utf8(output.stdout).context("ldconfig output was not UTF-8")?;
		for line in text.lines() {
			if !line.contains(soname) || !line.contains("=>") {
				continue;
			}
			if let Some((_, path_part)) = line.split_once("=>") {
				let candidate = PathBuf::from(path_part.trim());
				if candidate.exists() {
					return Ok(candidate);
				}
			}
		}
	}

	for base in ["/lib", "/lib64", "/usr/lib", "/usr/lib64", "/lib/x86_64-linux-gnu", "/usr/lib/x86_64-linux-gnu"] {
		let candidate = Path::new(base).join(soname);
		if candidate.exists() {
			return Ok(candidate);
		}
	}

	bail!("required shared object {} not found on host", soname)
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

fn run_cmd_with_env(
	cwd: &Path,
	program: &str,
	args: &[&str],
	tool_env: Option<&LocalToolEnv>,
) -> Result<()> {
	println!("> {} {}", program, args.join(" "));
	let mut cmd = Command::new(program);
	cmd.args(args).current_dir(cwd);

	if let Some(env) = tool_env {
		let current_path = std::env::var("PATH").unwrap_or_default();
		let composed_path = format!("{}:{}", env.tool_bin_dir.display(), current_path);
		let current_ld = std::env::var("LD_LIBRARY_PATH").unwrap_or_default();
		let composed_ld = if current_ld.is_empty() {
			env.tool_lib_dir.display().to_string()
		} else {
			format!("{}:{current_ld}", env.tool_lib_dir.display())
		};
		let include = env.tool_include_dir.display().to_string();
		let lib = env.tool_lib_dir.display().to_string();

		cmd.env("PATH", composed_path)
			.env("LD_LIBRARY_PATH", composed_ld)
			.env("BISON_PKGDATADIR", env.bison_pkg_data_dir.display().to_string())
			.env("M4", env.m4_bin.display().to_string())
			.env("CFLAGS", format!("-I{include}"))
			.env("HOSTCFLAGS", format!("-I{include}"))
			.env("LDFLAGS", format!("-L{lib}"))
			.env("HOSTLDFLAGS", format!("-L{lib}"));
	}

	let status = cmd
		.status()
		.with_context(|| format!("failed to spawn command: {program}"))?;
	if status.success() {
		Ok(())
	} else {
		bail!("command failed with status {status}: {} {}", program, args.join(" "))
	}
}

fn run_cmd_with_env_overrides(
	cwd: &Path,
	program: &str,
	args: &[&str],
	env_overrides: &[(&str, String)],
) -> Result<()> {
	println!("> {} {}", program, args.join(" "));
	let mut cmd = Command::new(program);
	cmd.args(args).current_dir(cwd);
	for (key, value) in env_overrides {
		cmd.env(key, value);
	}

	let status = cmd
		.status()
		.with_context(|| format!("failed to spawn command: {program}"))?;
	if status.success() {
		Ok(())
	} else {
		bail!("command failed with status {status}: {} {}", program, args.join(" "))
	}
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

	fn init_git_repo(path: &Path) {
		run_ok(path, "git", &["init", "-b", "main"]);
		run_ok(path, "git", &["config", "user.name", "Test User"]);
		run_ok(path, "git", &["config", "user.email", "test@example.invalid"]);
	}

	fn make_upstream_component_repo(name: &str, file_name: &str, body: &str) -> tempfile::TempDir {
		let upstream = tempfile::tempdir().expect("upstream tempdir");
		let root = upstream.path();
		init_git_repo(root);
		write(&root.join(file_name), body);
		run_ok(root, "git", &["add", "."]);
		run_ok(root, "git", &["commit", "-m", &format!("init {name}")]);
		upstream
	}

	#[test]
	fn path_safety_rejects_parent_dir() {
		let root = std::env::temp_dir().join("mattos-path-safety");
		let result = resolve_component_destination(&root, "../escape");
		assert!(result.is_err());
	}

	#[test]
	fn initial_import_refuses_meaningful_preexisting_files() {
		let tmp = tempfile::tempdir().expect("tempdir");
		let root = tmp.path();
		let destination = root.join("src/userland/grep");
		write(&destination.join("real.rs"), "fn main() {}\n");
		let result = assert_initial_destination_safe(&destination);
		assert!(result.is_err());
	}

	#[test]
	fn initial_import_allows_placeholder_only_destination() {
		let tmp = tempfile::tempdir().expect("tempdir");
		let root = tmp.path();
		let destination = root.join("src/userland/grep");
		write(&destination.join(".gitkeep"), "");
		write(&destination.join("README.md"), "placeholder\n");
		assert_initial_destination_safe(&destination).expect("placeholder-only destination should pass");
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
			destination_path: "src/kernel/linux".to_string(),
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
			path: "src/kernel/linux".to_string(),
			sync: "copy".to_string(),
		};
		import_component(root, &comp, false).expect("initial import");
		run_ok(root, "git", &["add", "."]);
		run_ok(root, "git", &["commit", "-m", "import"]);

		write(&root.join("src/kernel/linux/README"), "local\n");
		run_ok(root, "git", &["add", "src/kernel/linux/README"]);
		run_ok(root, "git", &["commit", "-m", "local edit"]);

		write(&upstream_root.join("README"), "upstream\n");
		run_ok(upstream_root, "git", &["add", "README"]);
		run_ok(upstream_root, "git", &["commit", "-m", "upstream edit"]);

		let result = import_component(root, &comp, true);
		assert!(result.is_err());

		let merged = fs::read_to_string(root.join("src/kernel/linux/README")).expect("read merged file");
		assert!(merged.contains("<<<<<<<"));
		assert!(merged.contains(">>>>>>>"));
	}

	#[test]
	fn unrelated_dirty_files_do_not_block_component_import() {
		let grep_upstream = make_upstream_component_repo("grep", "Cargo.toml", "[package]\nname='uu_grep'\nversion='0.1.0'\n");

		let workspace = tempfile::tempdir().expect("workspace tempdir");
		let root = workspace.path();
		init_git_repo(root);
		write(&root.join("README.md"), "base\n");
		run_ok(root, "git", &["add", "."]);
		run_ok(root, "git", &["commit", "-m", "init"]);

		write(
			&root.join("upstream/sources.toml"),
			&format!(
				"[[component]]\nname='grep'\nrepo='{}'\nbranch='main'\npath='src/userland/grep'\nsync='copy'\n",
				grep_upstream.path().display()
			),
		);
		write(&root.join("docs/dirty-note.md"), "unrelated dirty file\n");

		import_sources(root, false, Some("grep".to_string()), false).expect("import should succeed");
		assert!(root.join("src/userland/grep/Cargo.toml").exists());
	}

	#[test]
	fn dirty_other_component_does_not_block_selected_component_import() {
		let grep_upstream = make_upstream_component_repo("grep", "Cargo.toml", "[package]\nname='uu_grep'\nversion='0.1.0'\n");
		let sed_upstream = make_upstream_component_repo("sed", "Cargo.toml", "[package]\nname='sed'\nversion='0.1.0'\n");

		let workspace = tempfile::tempdir().expect("workspace tempdir");
		let root = workspace.path();
		init_git_repo(root);
		write(&root.join("README.md"), "repo\n");
		run_ok(root, "git", &["add", "."]);
		run_ok(root, "git", &["commit", "-m", "init"]);

		write(
			&root.join("upstream/sources.toml"),
			&format!(
				"[[component]]\nname='grep'\nrepo='{}'\nbranch='main'\npath='src/userland/grep'\nsync='copy'\n\n[[component]]\nname='sed'\nrepo='{}'\nbranch='main'\npath='src/userland/sed'\nsync='copy'\n",
				grep_upstream.path().display(),
				sed_upstream.path().display()
			),
		);

		write(&root.join("src/userland/sed/local.txt"), "dirty sed tree\n");
		import_sources(root, false, Some("grep".to_string()), false).expect("grep import should succeed");
		assert!(root.join("src/userland/grep/Cargo.toml").exists());
	}

	#[test]
	fn failed_initial_import_does_not_write_state_metadata() {
		let upstream = make_upstream_component_repo("grep", "Cargo.toml", "[package]\nname='uu_grep'\nversion='0.1.0'\n");

		let workspace = tempfile::tempdir().expect("workspace tempdir");
		let root = workspace.path();
		init_git_repo(root);
		write(&root.join("README.md"), "repo\n");
		run_ok(root, "git", &["add", "."]);
		run_ok(root, "git", &["commit", "-m", "init"]);

		let comp = ComponentDef {
			name: "grep".to_string(),
			repo: upstream.path().to_string_lossy().to_string(),
			branch: "main".to_string(),
			path: "src/userland/grep".to_string(),
			sync: "copy".to_string(),
		};

		write(&root.join("src/userland/grep/not-placeholder.txt"), "data\n");
		let result = import_component(root, &comp, false);
		assert!(result.is_err());
		assert!(read_sync_state(root, "grep").expect("read state").is_none());
	}

	#[test]
	fn failed_sync_conflict_does_not_advance_state_commit() {
		let upstream = tempfile::tempdir().expect("upstream tempdir");
		let upstream_root = upstream.path();
		init_git_repo(upstream_root);
		write(&upstream_root.join("README"), "base\n");
		run_ok(upstream_root, "git", &["add", "."]);
		run_ok(upstream_root, "git", &["commit", "-m", "base"]);

		let workspace = tempfile::tempdir().expect("workspace tempdir");
		let root = workspace.path();
		init_git_repo(root);
		write(&root.join("README.md"), "repo\n");
		run_ok(root, "git", &["add", "."]);
		run_ok(root, "git", &["commit", "-m", "init"]);

		let comp = ComponentDef {
			name: "grep".to_string(),
			repo: upstream_root.to_string_lossy().to_string(),
			branch: "main".to_string(),
			path: "src/userland/grep".to_string(),
			sync: "copy".to_string(),
		};

		import_component(root, &comp, false).expect("initial import");
		run_ok(root, "git", &["add", "."]);
		run_ok(root, "git", &["commit", "-m", "import"]);
		let before = read_sync_state(root, "grep")
			.expect("read state")
			.expect("present")
			.imported_commit;

		write(&root.join("src/userland/grep/README"), "local\n");
		run_ok(root, "git", &["add", "src/userland/grep/README"]);
		run_ok(root, "git", &["commit", "-m", "local"]);

		write(&upstream_root.join("README"), "upstream\n");
		run_ok(upstream_root, "git", &["add", "README"]);
		run_ok(upstream_root, "git", &["commit", "-m", "upstream"]);

		let result = import_component(root, &comp, true);
		assert!(result.is_err());
		let after = read_sync_state(root, "grep")
			.expect("read state")
			.expect("present")
			.imported_commit;
		assert_eq!(before, after);
	}

	#[test]
	fn sync_preserves_uncommitted_local_component_changes() {
		let upstream = tempfile::tempdir().expect("upstream tempdir");
		let upstream_root = upstream.path();
		init_git_repo(upstream_root);
		write(&upstream_root.join("README"), "base\n");
		run_ok(upstream_root, "git", &["add", "."]);
		run_ok(upstream_root, "git", &["commit", "-m", "base"]);

		let workspace = tempfile::tempdir().expect("workspace tempdir");
		let root = workspace.path();
		init_git_repo(root);
		write(&root.join("README.md"), "repo\n");
		run_ok(root, "git", &["add", "."]);
		run_ok(root, "git", &["commit", "-m", "init"]);

		let comp = ComponentDef {
			name: "grep".to_string(),
			repo: upstream_root.to_string_lossy().to_string(),
			branch: "main".to_string(),
			path: "src/userland/grep".to_string(),
			sync: "copy".to_string(),
		};

		import_component(root, &comp, false).expect("initial import");
		run_ok(root, "git", &["add", "."]);
		run_ok(root, "git", &["commit", "-m", "import"]);

		write(&upstream_root.join("NEWS"), "upstream change\n");
		run_ok(upstream_root, "git", &["add", "NEWS"]);
		run_ok(upstream_root, "git", &["commit", "-m", "news"]);

		write(&root.join("src/userland/grep/local-only.txt"), "local edit\n");
		import_component(root, &comp, true).expect("update should include local edits");

		assert_eq!(
			fs::read_to_string(root.join("src/userland/grep/local-only.txt")).expect("read local file"),
			"local edit\n"
		);
		assert_eq!(
			fs::read_to_string(root.join("src/userland/grep/NEWS")).expect("read upstream news"),
			"upstream change\n"
		);
	}

	#[test]
	fn path_safety_accepts_normal_relative_path() {
		let root = std::env::temp_dir().join("mattos-path-ok");
		let result = resolve_component_destination(&root, "src/kernel/linux").expect("valid path");
		assert!(result.ends_with(Path::new("src/kernel/linux")));
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
			path: "src/kernel/linux".to_string(),
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
				path: "src/kernel/linux".to_string(),
				sync: "copy".to_string(),
			},
			ComponentDef {
				name: "brush".to_string(),
				repo: "y".to_string(),
				branch: "main".to_string(),
				path: "src/userland/brush".to_string(),
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
				path: "src/kernel/linux".to_string(),
				sync: "copy".to_string(),
			},
			ComponentDef {
				name: "brush".to_string(),
				repo: "y".to_string(),
				branch: "main".to_string(),
				path: "src/userland/brush".to_string(),
				sync: "copy".to_string(),
			},
		];
		let selected = select_components(&components, true, None).expect("select all");
		assert_eq!(selected.len(), 2);
	}

	#[test]
	fn shell_escape_leaves_safe_text() {
		let escaped = shell_escape("src/kernel/linux");
		assert_eq!(escaped, "src/kernel/linux");
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
			path: "src/kernel/linux".to_string(),
			sync: "copy".to_string(),
		}];
		let selected = select_components(&components, false, Some("linux".to_string()))
			.expect("select linux");
		assert_eq!(selected[0].path, "src/kernel/linux");
	}

	#[test]
	fn read_sources_parses_components() {
		let tmp = tempfile::tempdir().expect("tempdir");
		let root = tmp.path();
		write(
			&root.join("upstream/sources.toml"),
			"[[component]]\nname='linux'\nrepo='https://example.invalid/linux.git'\nbranch='main'\npath='src/kernel/linux'\nsync='copy'\n",
		);
		let sources = read_sources(root).expect("read sources");
		assert_eq!(sources.component.len(), 1);
		assert_eq!(sources.component[0].name, "linux");
	}

	#[test]
	fn grub_source_validation_requires_authoritative_path() {
		let tmp = tempfile::tempdir().expect("tempdir");
		let result = validate_grub_config_source(tmp.path());
		assert!(result.is_err());
		let err = result.expect_err("missing source should fail").to_string();
		assert!(err.contains(AUTHORITATIVE_GRUB_CFG));
	}

	#[test]
	fn grub_source_validation_rejects_obsolete_duplicate_path() {
		let tmp = tempfile::tempdir().expect("tempdir");
		let root = tmp.path();
		write(
			&root.join(AUTHORITATIVE_GRUB_CFG),
			"menuentry \"MattOS (systemd)\" {}\nmenuentry \"MattOS (rescue init)\" {}\n",
		);
		write(&root.join(OBSOLETE_GRUB_CFG_PATHS[0]), "legacy duplicate\n");

		let result = validate_grub_config_source(root);
		assert!(result.is_err());
		let err = result.expect_err("duplicate should fail").to_string();
		assert!(err.contains(OBSOLETE_GRUB_CFG_PATHS[0]));
	}

	#[test]
	fn grub_source_validation_accepts_single_authoritative_path() {
		let tmp = tempfile::tempdir().expect("tempdir");
		let root = tmp.path();
		write(
			&root.join(AUTHORITATIVE_GRUB_CFG),
			"menuentry \"MattOS (systemd)\" {}\nmenuentry \"MattOS (rescue init)\" {}\n",
		);

		let source = validate_grub_config_source(root).expect("authoritative source should pass");
		assert!(source.ends_with(AUTHORITATIVE_GRUB_CFG));
	}

	#[test]
	fn staged_grub_validation_requires_normal_and_rescue_entries() {
		let tmp = tempfile::tempdir().expect("tempdir");
		let path = tmp.path().join("grub.cfg");
		write(
			&path,
			"set default=0\nmenuentry \"MattOS (systemd)\" { linux /boot/vmlinuz rdinit=/usr/lib/systemd/systemd }\n",
		);

		let result = validate_staged_grub_config(&path);
		assert!(result.is_err());
		let err = result.expect_err("missing rescue should fail").to_string();
		assert!(err.contains(GRUB_RESCUE_ENTRY));
	}

	#[test]
	fn staged_grub_validation_accepts_required_markers() {
		let tmp = tempfile::tempdir().expect("tempdir");
		let path = tmp.path().join("grub.cfg");
		write(
			&path,
			"menuentry \"MattOS (systemd)\" { linux /boot/vmlinuz rdinit=/usr/lib/systemd/systemd }\nmenuentry \"MattOS (rescue init)\" { linux /boot/vmlinuz rdinit=/usr/libexec/mattos/rescue-init }\n",
		);

		validate_staged_grub_config(&path).expect("valid staged config should pass");
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
			destination_path: "src/userland/brush".to_string(),
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
			path: "src/kernel/linux".to_string(),
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
		let resolved = resolve_component_destination(&root, "src/userland/brush").expect("resolve");
		assert!(resolved.ends_with("src/userland/brush"));
	}

	#[test]
	fn source_selection_all_ignores_component_flag() {
		let components = vec![ComponentDef {
			name: "linux".to_string(),
			repo: "x".to_string(),
			branch: "main".to_string(),
			path: "src/kernel/linux".to_string(),
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

	#[test]
	fn read_sources_parses_systemd_component() {
		let tmp = tempfile::tempdir().expect("tempdir");
		let root = tmp.path();
		write(
			&root.join("upstream/sources.toml"),
			"[[component]]\nname='systemd'\nrepo='https://github.com/systemd/systemd.git'\nbranch='main'\npath='src/system/systemd'\nsync='copy'\n",
		);
		let sources = read_sources(root).expect("read sources");
		assert_eq!(sources.component.len(), 1);
		assert_eq!(sources.component[0].name, "systemd");
		assert_eq!(sources.component[0].path, "src/system/systemd");
	}

	#[test]
	fn systemd_import_destination_is_safe() {
		let root = std::env::temp_dir().join("mattos-systemd-path");
		let safe = resolve_component_destination(&root, "src/system/systemd").expect("resolve");
		assert!(safe.ends_with("src/system/systemd"));
		assert!(resolve_component_destination(&root, "src/system/../escape").is_err());
	}

	#[test]
	fn systemd_initial_import_writes_state() {
		let upstream = tempfile::tempdir().expect("upstream tempdir");
		let upstream_root = upstream.path();
		run_ok(upstream_root, "git", &["init", "-b", "main"]);
		run_ok(upstream_root, "git", &["config", "user.name", "Upstream User"]);
		run_ok(
			upstream_root,
			"git",
			&["config", "user.email", "upstream@example.invalid"],
		);
		write(&upstream_root.join("meson.build"), "project('systemd', 'c')\n");
		run_ok(upstream_root, "git", &["add", "."]);
		run_ok(upstream_root, "git", &["commit", "-m", "init"]);

		let workspace = tempfile::tempdir().expect("workspace tempdir");
		let root = workspace.path();
		run_ok(root, "git", &["init"]);
		run_ok(root, "git", &["config", "user.name", "MattOS User"]);
		run_ok(root, "git", &["config", "user.email", "mattos@example.invalid"]);
		write(&root.join("README.md"), "repo\n");
		run_ok(root, "git", &["add", "."]);
		run_ok(root, "git", &["commit", "-m", "init"]);

		let comp = ComponentDef {
			name: "systemd".to_string(),
			repo: upstream_root.to_string_lossy().to_string(),
			branch: "main".to_string(),
			path: "src/system/systemd".to_string(),
			sync: "copy".to_string(),
		};
		import_component(root, &comp, false).expect("initial import");
		assert!(root.join("src/system/systemd/meson.build").exists());

		let state = read_sync_state(root, "systemd")
			.expect("read state")
			.expect("state exists");
		assert_eq!(state.component, "systemd");
		assert_eq!(state.repo, comp.repo);
		assert_eq!(state.destination_path, "src/system/systemd");
	}

	#[test]
	fn systemd_sync_preserves_local_modifications() {
		let upstream = tempfile::tempdir().expect("upstream tempdir");
		let upstream_root = upstream.path();
		run_ok(upstream_root, "git", &["init", "-b", "main"]);
		run_ok(upstream_root, "git", &["config", "user.name", "Upstream User"]);
		run_ok(
			upstream_root,
			"git",
			&["config", "user.email", "upstream@example.invalid"],
		);
		write(&upstream_root.join("meson.build"), "base\n");
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
			name: "systemd".to_string(),
			repo: upstream_root.to_string_lossy().to_string(),
			branch: "main".to_string(),
			path: "src/system/systemd".to_string(),
			sync: "copy".to_string(),
		};
		import_component(root, &comp, false).expect("initial import");
		run_ok(root, "git", &["add", "."]);
		run_ok(root, "git", &["commit", "-m", "import"]);

		write(&root.join("src/system/systemd/meson.build"), "local change\n");
		run_ok(root, "git", &["add", "src/system/systemd/meson.build"]);
		run_ok(root, "git", &["commit", "-m", "local"]);

		write(&upstream_root.join("README"), "upstream only\n");
		run_ok(upstream_root, "git", &["add", "README"]);
		run_ok(upstream_root, "git", &["commit", "-m", "upstream"]);

		import_component(root, &comp, true).expect("update without conflict");
		let local = fs::read_to_string(root.join("src/system/systemd/meson.build")).expect("read local file");
		assert_eq!(local, "local change\n");
	}

	#[test]
	fn systemd_sync_conflict_behavior_surfaces_markers() {
		let upstream = tempfile::tempdir().expect("upstream tempdir");
		let upstream_root = upstream.path();
		run_ok(upstream_root, "git", &["init", "-b", "main"]);
		run_ok(upstream_root, "git", &["config", "user.name", "Upstream User"]);
		run_ok(
			upstream_root,
			"git",
			&["config", "user.email", "upstream@example.invalid"],
		);
		write(&upstream_root.join("meson.build"), "base\n");
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
			name: "systemd".to_string(),
			repo: upstream_root.to_string_lossy().to_string(),
			branch: "main".to_string(),
			path: "src/system/systemd".to_string(),
			sync: "copy".to_string(),
		};
		import_component(root, &comp, false).expect("initial import");
		run_ok(root, "git", &["add", "."]);
		run_ok(root, "git", &["commit", "-m", "import"]);

		write(&root.join("src/system/systemd/meson.build"), "local\n");
		run_ok(root, "git", &["add", "src/system/systemd/meson.build"]);
		run_ok(root, "git", &["commit", "-m", "local"]);

		write(&upstream_root.join("meson.build"), "upstream\n");
		run_ok(upstream_root, "git", &["add", "meson.build"]);
		run_ok(upstream_root, "git", &["commit", "-m", "upstream"]);

		let result = import_component(root, &comp, true);
		assert!(result.is_err());
		let merged = fs::read_to_string(root.join("src/system/systemd/meson.build")).expect("read merged");
		assert!(merged.contains("<<<<<<<"));
		assert!(merged.contains(">>>>>>>"));
	}

	#[test]
	fn build_plan_all_includes_uutils_stages() {
		let plan = build_plan(BuildStage::All);
		assert_eq!(plan[0], BuildStage::Kernel);
		assert!(plan.contains(&BuildStage::Grep));
		assert!(plan.contains(&BuildStage::Sed));
		assert!(plan.contains(&BuildStage::Findutils));
		assert!(plan.contains(&BuildStage::Diffutils));
		assert!(plan.contains(&BuildStage::Pam));
		assert!(plan.contains(&BuildStage::Shadow));
		assert!(plan.contains(&BuildStage::SudoRs));
		assert_eq!(plan.last().copied(), Some(BuildStage::Iso));
	}

	#[test]
	fn account_database_validation_accepts_live_profile_shape() {
		let tmp = tempfile::tempdir().expect("tempdir");
		let root = tmp.path();
		write(
			&root.join("etc/passwd"),
			"root:x:0:0:root:/root:/bin/brush\nmattos:x:1000:1000:MattOS Live User:/home/mattos:/bin/brush\n",
		);
		write(
			&root.join("etc/group"),
			"root:x:0:\nsudo:x:27:mattos\nmattos:x:1000:\n",
		);
		write(&root.join("etc/shadow"), "root:!:::::::\nmattos:!:::::::\n");
		write(&root.join("etc/gshadow"), "root:!::\nsudo:!::mattos\nmattos:!::\n");

		validate_account_database(root).expect("valid live account database should pass");
	}

	#[test]
	fn account_database_validation_rejects_duplicate_uid() {
		let tmp = tempfile::tempdir().expect("tempdir");
		let root = tmp.path();
		write(
			&root.join("etc/passwd"),
			"root:x:0:0:root:/root:/bin/brush\nmattos:x:0:1000:MattOS Live User:/home/mattos:/bin/brush\n",
		);
		write(
			&root.join("etc/group"),
			"root:x:0:\nsudo:x:27:mattos\nmattos:x:1000:\n",
		);
		write(&root.join("etc/shadow"), "root:!:::::::\nmattos:!:::::::\n");
		write(&root.join("etc/gshadow"), "root:!::\nsudo:!::mattos\nmattos:!::\n");

		let result = validate_account_database(root);
		assert!(result.is_err());
	}

	#[test]
	#[cfg(unix)]
	fn enforce_auth_file_modes_sets_secure_permissions() {
		use std::os::unix::fs::PermissionsExt;

		let tmp = tempfile::tempdir().expect("tempdir");
		let root = tmp.path();
		for rel in [
			"etc/shadow",
			"etc/gshadow",
			"etc/passwd",
			"etc/group",
			"etc/sudoers",
			"etc/sudoers.d/00-mattos-live",
			"etc/sudoers.d/README",
			"usr/bin/login",
			"usr/bin/su",
			"usr/bin/passwd",
			"usr/bin/sudo",
		] {
			write(&root.join(rel), "x\n");
		}
		fs::create_dir_all(root.join("root")).expect("root dir");
		fs::create_dir_all(root.join("home/mattos")).expect("home dir");

		enforce_auth_file_modes(root).expect("set modes");

		let sudo_mode = fs::metadata(root.join("usr/bin/sudo"))
			.expect("sudo metadata")
			.permissions()
			.mode()
			& 0o7777;
		assert_eq!(sudo_mode, 0o4755);

		let shadow_mode = fs::metadata(root.join("etc/shadow"))
			.expect("shadow metadata")
			.permissions()
			.mode()
			& 0o7777;
		assert_eq!(shadow_mode, 0o600);
	}

	#[test]
	fn duplicate_command_detection_flags_conflicts() {
		let mut providers = BTreeMap::<&str, Vec<String>>::new();
		providers.insert(COREUTILS_PROVIDER, vec!["cat".to_string()]);
		providers.insert(GREP_PROVIDER, vec!["cat".to_string()]);
		let result = validate_no_duplicate_commands(&providers);
		assert!(result.is_err());
	}

	#[test]
	fn duplicate_command_detection_allows_unique_set() {
		let mut providers = BTreeMap::<&str, Vec<String>>::new();
		providers.insert(COREUTILS_PROVIDER, vec!["cat".to_string()]);
		providers.insert(GREP_PROVIDER, vec!["grep".to_string()]);
		validate_no_duplicate_commands(&providers).expect("unique set should pass");
	}

	#[test]
	fn install_userland_binary_reports_missing_executable() {
		let tmp = tempfile::tempdir().expect("tempdir");
		let root = tmp.path();
		let rootfs = root.join("rootfs");
		fs::create_dir_all(root.join("src/userland/grep/target/release")).expect("mkdir");

		let spec = BinaryInstallSpec {
			provider: GREP_PROVIDER,
			source_rel: "src/userland/grep/target/release/grep",
			install_name: "grep",
			command_name: "grep",
		};
		let result = install_userland_binary(root, &rootfs, &spec);
		assert!(result.is_err());
	}

	#[test]
	fn userland_inventory_writer_emits_sections() {
		let tmp = tempfile::tempdir().expect("tempdir");
		let root = tmp.path();
		let mut inventory = UserlandInventory::default();
		inventory.add_implemented(COREUTILS_PROVIDER, "cat");
		inventory.add_compiled(COREUTILS_PROVIDER, "cat");
		inventory.add_installed(COREUTILS_PROVIDER, "cat");
		inventory.add_excluded(DIFFUTILS_PROVIDER, "sdiff");
		inventory.add_failed(DIFFUTILS_PROVIDER, "diff3", "not implemented upstream");

		write_userland_inventory(root, &inventory).expect("write inventory");
		let body = fs::read_to_string(root.join(USERLAND_INVENTORY_PATH)).expect("read inventory");
		assert!(body.contains("[implemented_upstream]"));
		assert!(body.contains("uutils/coreutils:cat"));
		assert!(body.contains("[failed_compatibility]"));
	}

	#[test]
	fn read_sources_parses_uutils_component_set() {
		let tmp = tempfile::tempdir().expect("tempdir");
		let root = tmp.path();
		write(
			&root.join("upstream/sources.toml"),
			"[[component]]\nname='grep'\nrepo='https://github.com/uutils/grep.git'\nbranch='main'\npath='src/userland/grep'\nsync='copy'\n\n[[component]]\nname='sed'\nrepo='https://github.com/uutils/sed.git'\nbranch='main'\npath='src/userland/sed'\nsync='copy'\n",
		);
		let sources = read_sources(root).expect("read sources");
		assert_eq!(sources.component.len(), 2);
		assert_eq!(sources.component[0].name, "grep");
		assert_eq!(sources.component[1].name, "sed");
	}

}
