import subprocess
import sys
import tempfile
import unittest
import importlib.util
import json
from io import StringIO
from pathlib import Path
from unittest.mock import MagicMock, patch


sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))

from packages.catalog import load_catalog, load_package, load_profile, load_profiles
from packages import cli
from packages.cli import build_command_plan
from packages.executor import _nodesource_nodejs_candidate, ensure_nodejs_npm, execution_lock, run_shell_installer
from packages.models import NodejsOperation, PackageDefinition, PackageTarget, ProfileDefinition, ProfilePackage, ScriptDependencies, ScriptOperation, ShellInstallerOperation
from packages.planner import PackageResolutionError, resolve_profiles
from packages.providers import plan_execution_steps, plan_provider_operations, preferred_provider
from server.btrfs_snapshots import BtrfsSnapshotManager
from storage_smb import LEGACY_HELPER_PATH, MountConfiguration, install_prerequisites, mount_from_config, retire_legacy_implementation, service_contents, sudo
from host import HostPlatform
from system import LinuxDistro, PackageManager, detect_package_platform
from konsave.apply import choose_profile


def load_tailscale_configure_script():
    """Load the source-owned Tailscale hook without executing its main block."""

    path = Path(__file__).resolve().parents[1] / "src" / "scripts" / "configure_tailscale.py"
    specification = importlib.util.spec_from_file_location("configure_tailscale", path)
    module = importlib.util.module_from_spec(specification)
    assert specification.loader is not None
    specification.loader.exec_module(module)
    return module


def load_setup_script(name: str):
    """Load one package setup hook without running its script entry point."""

    path = Path(__file__).resolve().parents[1] / "src" / "scripts" / name
    specification = importlib.util.spec_from_file_location(path.stem, path)
    module = importlib.util.module_from_spec(specification)
    assert specification.loader is not None
    specification.loader.exec_module(module)
    return module


class PackagePlanningTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        root = Path(__file__).resolve().parents[1]
        cls.catalog = load_catalog(root / "resources" / "packages")
        cls.profiles = load_profiles(root / "resources" / "profiles")

    def test_complete_desktop_orders_dependencies_before_dependents(self):
        plan = resolve_profiles(["complete-desktop"], self.catalog, self.profiles, "linux", ("apt",))
        self.assertEqual(
            [package.name for package in plan.packages],
            [
                "git", "curl", "ripgrep", "openssh-server", "fastfetch", "tailscale", "qdirstat", "baobab", "kate", "konsole", "dolphin",
                "flatpak", "mission-center", "rustdesk", "snapd", "bitwarden", "bw", "discord", "variety", "papirus-icon-theme",
                "github-cli", "codex-cli", "vscode", "kmines", "steam", "libreoffice", "pipx", "konsave",
            ],
        )

    def test_mattos_uses_only_explicit_mattos_profile_and_package_targets(self):
        plan = resolve_profiles(["complete-desktop"], self.catalog, self.profiles, "mattos", ("apt",))
        self.assertEqual(
            [package.name for package in plan.packages],
            ["git", "curl", "ripgrep", "snapd", "bitwarden", "bw", "flatpak", "discord", "github-cli", "codex-cli", "basalt"],
        )
        platforms_by_package = {package.name: package.target.platform for package in plan.packages}
        self.assertEqual(platforms_by_package["basalt"], "mattos")
        self.assertTrue(all(platform == "mattos" for platform in platforms_by_package.values()))
        self.assertNotIn("qdirstat", platforms_by_package)
        self.assertNotIn("steam", platforms_by_package)
        self.assertNotIn("libreoffice", platforms_by_package)
        self.assertIn("configure_cosmic_wallpapers.py", plan.profile_scripts)

    def test_platform_profile_scripts_are_excluded_on_other_platforms(self):
        mattos_plan = resolve_profiles(["desktop"], self.catalog, self.profiles, "mattos", ("apt",))
        linux_plan = resolve_profiles(["desktop"], self.catalog, self.profiles, "linux", ("apt",))
        self.assertEqual(mattos_plan.profile_scripts, ("configure_cosmic_wallpapers.py",))
        self.assertEqual(linux_plan.profile_scripts, ())

    def test_mattos_is_detected_as_an_apt_platform(self):
        platform_name = detect_package_platform(
            HostPlatform("linux", "x86_64", "x86_64"),
            LinuxDistro("mattos", "MattOS", "1.0", ("debian",)),
        )
        self.assertEqual(platform_name, "mattos")

    def test_mattos_does_not_fall_back_to_linux_target(self):
        catalog = {
            "mattos-tool": PackageDefinition(
                "mattos-tool",
                "Test MattOS package",
                (),
                ScriptDependencies((), ()),
                (
                    PackageTarget("linux", "apt", "linux-tool", (), {}),
                ),
            )
        }
        profiles = {
            "mattos-test": ProfileDefinition(
                "mattos-test",
                "Test MattOS profile",
                (),
                (),
                {
                    "mattos": (ProfilePackage("mattos-tool", True),),
                },
                (),
            )
        }
        with self.assertRaisesRegex(PackageResolutionError, "No package target is defined for mattos."):
            resolve_profiles(["mattos-test"], catalog, profiles, "mattos", ("apt",))

    def test_required_mattos_package_reports_incompatible_manager(self):
        with self.assertRaisesRegex(
            PackageResolutionError,
            "Package 'git' is not available for the dnf package manager on mattos.",
        ):
            resolve_profiles(["gaming"], self.catalog, self.profiles, "mattos", ("dnf",))

    def test_windows_excludes_linux_only_profile_packages(self):
        plan = resolve_profiles(["complete-desktop"], self.catalog, self.profiles, "windows")
        self.assertEqual(
            [package.name for package in plan.packages],
            ["git", "curl", "ripgrep", "bitwarden", "bw", "discord", "github-cli", "npm", "codex-cli"],
        )
        self.assertEqual(plan.skipped, {})
        self.assertNotIn("flatpak", [package.name for package in plan.packages])
        self.assertNotIn("konsave", [package.name for package in plan.packages])
        self.assertNotIn("steam", [package.name for package in plan.packages])
        self.assertNotIn("libreoffice", [package.name for package in plan.packages])
        self.assertNotIn("mission-center", [package.name for package in plan.packages])

    def test_required_package_without_target_is_rejected(self):
        with self.assertRaises(PackageResolutionError):
            resolve_profiles(["desktop"], self.catalog, self.profiles, "linux", ("apk",))

    def test_dnf_does_not_fall_back_to_apt_target(self):
        with self.assertRaisesRegex(
            PackageResolutionError,
            "Package 'fastfetch' is not available for the dnf package manager on linux.",
        ):
            resolve_profiles(
                ["complete-desktop"],
                self.catalog,
                self.profiles,
                "linux",
                (preferred_provider(PackageManager.DNF),),
            )

    def test_provider_plan_batches_apt_packages(self):
        plan = resolve_profiles(["complete-desktop"], self.catalog, self.profiles, "linux", ("apt",))
        operations = plan_provider_operations(plan.packages, PackageManager.APT)
        self.assertEqual(operations[0].commands[1].argv[:3], ("apt-get", "install", "-y"))
        self.assertIn("pipx", operations[0].commands[1].argv)

    def test_coding_uses_the_codex_shell_installer_without_npm(self):
        plan = resolve_profiles(["coding"], self.catalog, self.profiles, "linux", ("apt",))
        codex = next(package for package in plan.packages if package.name == "codex-cli")
        self.assertEqual(codex.target.provider, "shell_installer")
        self.assertNotIn("npm", [package.name for package in plan.packages])
        steps = plan_execution_steps(plan.packages, plan.profile_scripts, PackageManager.APT)
        self.assertIn(ShellInstallerOperation(("codex-cli",), ("https://chatgpt.com/codex/install.sh",)), steps)

    def test_nodesource_candidate_detection_uses_the_selected_version(self):
        self.assertTrue(
            _nodesource_nodejs_candidate(
                "nodejs:\n  Candidate: 24.19.0-1nodesource1\n  Version table:\n     24.19.0-1nodesource1 600\n        500 https://deb.nodesource.com/node_24.x nodistro/main amd64 Packages\n     20.19.4+dfsg-1 500\n        500 http://archive.ubuntu.com/ubuntu questing/universe amd64 Packages\n"
            )
        )
        self.assertFalse(
            _nodesource_nodejs_candidate(
                "nodejs:\n  Candidate: 20.19.4+dfsg-1\n  Version table:\n     24.19.0-1nodesource1 500\n        500 https://deb.nodesource.com/node_24.x nodistro/main amd64 Packages\n     20.19.4+dfsg-1 600\n        500 http://archive.ubuntu.com/ubuntu questing/universe amd64 Packages\n"
            )
        )

    def test_nodejs_capability_skips_apt_when_node_and_npm_work(self):
        with patch("packages.executor._working_command", return_value=True), patch("packages.executor.run_command") as run_command:
            ensure_nodejs_npm()
        run_command.assert_not_called()

    def test_nodejs_capability_installs_the_nodesource_candidate_when_needed(self):
        policy = "nodejs:\n  Candidate: 24.19.0-1nodesource1\n  Version table:\n     24.19.0-1nodesource1 600\n        500 https://deb.nodesource.com/node_24.x nodistro/main amd64 Packages\n"
        result = subprocess.CompletedProcess(("apt-cache", "policy", "nodejs"), 0, policy, "")
        with patch("packages.executor._working_command", side_effect=(False, True, True)), patch(
            "packages.executor._command_with_privileges", side_effect=lambda command: command.argv
        ), patch("packages.executor.run_command", return_value=result) as run_command:
            ensure_nodejs_npm()
        commands = [call.args[0] for call in run_command.call_args_list]
        self.assertIn(("apt-get", "install", "-y", "nodejs"), commands)
        self.assertNotIn(("apt-get", "install", "-y", "npm"), commands)

    def test_nodejs_capability_installs_distribution_npm_without_nodesource(self):
        policy = "nodejs:\n  Candidate: 20.19.4+dfsg-1\n  Version table:\n     20.19.4+dfsg-1 500\n        500 http://archive.ubuntu.com/ubuntu questing/universe amd64 Packages\n"
        result = subprocess.CompletedProcess(("apt-cache", "policy", "nodejs"), 0, policy, "")
        with patch("packages.executor._working_command", side_effect=(False, True, True)), patch(
            "packages.executor._command_with_privileges", side_effect=lambda command: command.argv
        ), patch("packages.executor.run_command", return_value=result) as run_command:
            ensure_nodejs_npm()
        commands = [call.args[0] for call in run_command.call_args_list]
        self.assertIn(("apt-get", "install", "-y", "npm"), commands)

    def test_shell_installer_runs_a_private_download_with_sh(self):
        url = "https://example.com/install.sh"
        response = MagicMock()
        response.geturl.return_value = url
        response.read.side_effect = (b"echo installed\n", b"")
        download = MagicMock()
        download.__enter__.return_value = response
        with patch("packages.executor.urlopen", return_value=download) as urlopen, patch(
            "packages.executor.run_command"
        ) as run_command:
            run_shell_installer(url)
        request = urlopen.call_args.args[0]
        self.assertEqual(request.full_url, url)
        self.assertEqual(request.get_header("User-agent"), "curl/8.5.0")
        self.assertEqual(urlopen.call_args.kwargs, {"timeout": 60})
        command = run_command.call_args.args[0]
        self.assertEqual(command[0], "sh")
        self.assertFalse(Path(command[1]).exists())

    def test_command_plan_builds_from_the_source_cli_module(self):
        root = Path(__file__).resolve().parents[1]
        _, platform_name, package_manager, package_plan, _ = build_command_plan(
            root,
            ("server",),
            platform_name="linux",
            package_manager=PackageManager.APT,
        )
        self.assertEqual(platform_name, "linux")
        self.assertEqual(package_manager, PackageManager.APT)
        self.assertEqual(package_plan.profiles[-1], "server")

    @unittest.skipIf(sys.platform == "win32", "POSIX file locking is not used on Windows.")
    def test_package_execution_lock_rejects_a_second_apply(self):
        root = Path(__file__).resolve().parents[1]
        with execution_lock(root):
            with self.assertRaisesRegex(RuntimeError, "Another package apply is already running"):
                with execution_lock(root):
                    pass

    def test_cli_reports_provider_failure_with_the_process_exit_code(self):
        result = (object(), "linux", PackageManager.APT, object(), ())
        with patch.object(cli, "build_command_plan", return_value=result), patch.object(cli, "print_plan"), patch.object(
            cli, "execute_operations", side_effect=subprocess.CalledProcessError(23, ("apt-get", "install"))
        ), patch.object(cli.sys, "stderr", new_callable=StringIO) as error_output:
            self.assertEqual(cli.main(("apply", "desktop", "--yes")), 23)
        self.assertIn("Error: package apply failed:", error_output.getvalue())

    def test_coding_scripts_run_before_profile_and_codex_install(self):
        plan = resolve_profiles(["coding"], self.catalog, self.profiles, "linux", ("apt",))
        steps = plan_execution_steps(plan.packages, plan.profile_scripts, PackageManager.APT)
        self.assertEqual(plan.profile_scripts, ("hello_world.py",))
        self.assertIsInstance(steps[0], ScriptOperation)
        self.assertEqual(steps[0].description, "Run profile dependency script 'hello_world.py'")
        codex_step = next(index for index, step in enumerate(steps) if getattr(step, "packages", ()) == ("codex-cli",))
        self.assertIsInstance(steps[codex_step - 1], ScriptOperation)
        self.assertEqual(steps[codex_step - 1].description, "Run pre-install script for 'codex-cli': hello_world.py")

    def test_remote_access_packages_use_ordered_linux_setup_hooks(self):
        plan = resolve_profiles(["desktop"], self.catalog, self.profiles, "linux", ("apt",))
        steps = plan_execution_steps(plan.packages, plan.profile_scripts, PackageManager.APT, plan.delete_packages)
        tailscale_step = next(index for index, step in enumerate(steps) if getattr(step, "packages", ()) == ("tailscale",))
        rustdesk_step = next(index for index, step in enumerate(steps) if getattr(step, "packages", ()) == ("rustdesk",))
        self.assertEqual(steps[tailscale_step - 1].script, "setup_tailscale_repository.py")
        self.assertEqual(steps[tailscale_step + 1].script, "configure_tailscale.py")
        self.assertEqual(steps[rustdesk_step - 1].script, "download_rustdesk.py")
        self.assertEqual(steps[rustdesk_step + 1].script, "configure_rustdesk.py")
        self.assertEqual(steps[rustdesk_step].provider, "apt_deb")

    def test_linux_openssh_and_variety_use_post_install_hooks(self):
        plan = resolve_profiles(["desktop"], self.catalog, self.profiles, "linux", ("apt",))
        steps = plan_execution_steps(plan.packages, plan.profile_scripts, PackageManager.APT, plan.delete_packages)
        ssh_step = next(index for index, step in enumerate(steps) if getattr(step, "packages", ()) == ("openssh-server",))
        variety_step = next(index for index, step in enumerate(steps) if getattr(step, "packages", ()) == ("variety",))
        self.assertEqual(steps[ssh_step + 1].script, "configure_openssh_server.py")
        self.assertEqual(steps[variety_step + 1].script, "configure_variety.py")

    def test_konsave_uses_a_post_install_profile_workflow(self):
        plan = resolve_profiles(["complete-desktop"], self.catalog, self.profiles, "linux", ("apt", "pipx"))
        steps = plan_execution_steps(plan.packages, plan.profile_scripts, PackageManager.APT, plan.delete_packages)
        konsave_step = next(index for index, step in enumerate(steps) if getattr(step, "packages", ()) == ("konsave",))
        self.assertEqual(steps[konsave_step + 1].script, "configure_konsave.py")

    def test_konsave_profile_menu_preserves_legacy_default_and_skip_choice(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            profiles = root / "resources" / "KDEProfiles"
            profiles.mkdir(parents=True)
            (profiles / "HungLoStandard.knsv").write_text("profile", encoding="utf-8")
            (profiles / "Other.knsv").write_text("profile", encoding="utf-8")
            with patch("builtins.input", return_value=""):
                self.assertEqual(choose_profile(root), "HungLoStandard")
            with patch("builtins.input", return_value="1"):
                self.assertIsNone(choose_profile(root))

    def test_openssh_hook_enables_the_legacy_ssh_service(self):
        configure_openssh = load_setup_script("configure_openssh_server.py")
        with patch.object(configure_openssh.subprocess, "run") as run_command, patch.object(configure_openssh.os, "geteuid", return_value=1000):
            run_command.return_value.returncode = 0
            self.assertEqual(configure_openssh.main(), 0)
        run_command.assert_called_once_with(("sudo", "systemctl", "enable", "--now", "ssh"), check=False)

    def test_variety_hook_copies_the_repository_template(self):
        configure_variety = load_setup_script("configure_variety.py")
        self.assertTrue(configure_variety.SOURCE_CONFIGURATION.is_file())
        self.assertIn("/mnt/storage/OneDrive/Media/Wallpapers/Wide", configure_variety.SOURCE_CONFIGURATION.read_text(encoding="utf-8"))
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / "variety.conf"
            source.write_text("configured=true\n", encoding="utf-8")
            account = type("Account", (), {"pw_dir": str(root / "home"), "pw_uid": 1000, "pw_gid": 1000})()
            with patch.object(configure_variety.os, "geteuid", return_value=1000):
                destination = configure_variety.deploy_configuration(source, account)
        self.assertEqual(destination.name, "variety.conf")
        self.assertEqual(destination.parent.name, "variety")

    def test_cosmic_wallpaper_hook_writes_native_settings(self):
        configure_cosmic = load_setup_script("configure_cosmic_wallpapers.py")
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            home = root / "home"
            account = type("Account", (), {"pw_dir": str(home), "pw_uid": 1000, "pw_gid": 1000})()
            wallpaper_directory = Path("/mnt/storage/OneDrive/Media/Wallpapers/Wide")
            configure_cosmic.configure(home, wallpaper_directory, account)

            background = home / ".config/cosmic/com.system76.CosmicBackground/v1/all"
            current_folder = home / ".config/cosmic/com.system76.CosmicSettings.Wallpaper/v1/current-folder"
            recent_folders = home / ".config/cosmic/com.system76.CosmicSettings.Wallpaper/v1/recent-folders"
            self.assertIn('source: Path("/mnt/storage/OneDrive/Media/Wallpapers/Wide")', background.read_text(encoding="utf-8"))
            self.assertEqual(current_folder.read_text(encoding="utf-8"), 'Some("/mnt/storage/OneDrive/Media/Wallpapers/Wide")')
            self.assertEqual(json.loads(recent_folders.read_text(encoding="utf-8")), [str(wallpaper_directory)])

    def test_tailscale_hook_skips_interactive_enrollment_when_connected(self):
        configure_tailscale = load_tailscale_configure_script()
        connected_status = {"BackendState": "Running", "Self": {"Online": True}}
        self.assertTrue(configure_tailscale.is_connected(connected_status))
        self.assertFalse(configure_tailscale.is_connected({"BackendState": "Running", "Self": {"Online": False}}))
        with patch.object(configure_tailscale, "tailscale_status", return_value=connected_status), patch.object(
            configure_tailscale.subprocess,
            "run",
        ) as run_command, patch.object(configure_tailscale, "confirm_enrollment") as confirm_enrollment:
            self.assertEqual(configure_tailscale.main(), 0)
        self.assertEqual(run_command.call_count, 1)
        confirm_enrollment.assert_not_called()

    def test_storage_mount_service_retries_the_python_helper(self):
        configuration = MountConfiguration("100.72.33.98", "storage", "/mnt/storage", "/etc/samba/credentials-storage-matt", 1000, 1000)
        self.assertEqual(configuration.server, "100.72.33.98")
        unit = service_contents()
        self.assertIn("ExecStart=/usr/bin/python3 /usr/local/lib/linuxscripts/storage_smb_mount.py --mount --config /etc/linuxscripts/storage-smb-mount.json", unit)
        self.assertIn("Restart=on-failure", unit)
        self.assertIn("RestartSec=20", unit)
        self.assertIn("StartLimitIntervalSec=0", unit)

    def test_storage_mount_retires_legacy_helper_without_removing_active_credentials(self):
        with patch("storage_smb.sudo") as run_privileged:
            retire_legacy_implementation("matt", Path("/etc/samba/credentials-storage-matt"))
        commands = [call.args[0] for call in run_privileged.call_args_list]
        self.assertIn(("systemctl", "disable", "--now", "storage-smb-mount.service"), commands)
        self.assertIn(("rm", "-f", str(LEGACY_HELPER_PATH)), commands)
        self.assertNotIn(("rm", "-f", "/etc/samba/credentials-storage-matt"), commands)

    def test_storage_mount_privilege_wrapper_allows_nonfatal_commands(self):
        with patch("storage_smb.subprocess.run") as run_command:
            sudo(("systemctl", "disable", "--now", "storage-smb-mount.service"), check=False)
        self.assertFalse(run_command.call_args.kwargs["check"])

    def test_storage_mount_prerequisites_do_not_refresh_unrelated_apt_sources(self):
        with patch("storage_smb.sudo") as run_privileged:
            install_prerequisites()
        self.assertEqual(
            [call.args[0] for call in run_privileged.call_args_list],
            [("apt-get", "install", "-y", "smbclient", "cifs-utils")],
        )

    def test_storage_mount_checks_for_an_exact_mountpoint(self):
        configuration = MountConfiguration("100.72.33.98", "storage", "/mnt/storage", "/etc/samba/credentials-storage-matt", 1000, 1000)
        unmounted = type("Result", (), {"returncode": 1, "stdout": ""})()
        with patch("storage_smb.tailscale_connected", return_value=True), patch("storage_smb.subprocess.run", return_value=unmounted) as run_command:
            self.assertEqual(mount_from_config(configuration), 0)
        self.assertEqual(
            run_command.call_args_list[0].args[0],
            ("findmnt", "-rn", "--mountpoint", "/mnt/storage", "-o", "SOURCE"),
        )

    def test_btrfs_snapshot_manager_parses_legacy_subvolume_output(self):
        output = "ID 257 gen 10 parent 5 top level 5 path snapshots/@data-2026-08-06-1200\n"
        self.assertEqual(
            BtrfsSnapshotManager.parse_subvolumes(output),
            [("257", "5", "snapshots/@data-2026-08-06-1200")],
        )
        manager = BtrfsSnapshotManager.with_defaults()
        with patch.object(BtrfsSnapshotManager, "run") as run_command:
            run_command.return_value.stdout = output
            run_command.return_value.returncode = 0
            self.assertEqual(manager.snapshot_entries(), ["snapshots/@data-2026-08-06-1200"])

    def test_linux_profile_removals_run_after_installations(self):
        plan = resolve_profiles(["gaming"], self.catalog, self.profiles, "linux", ("apt",))
        self.assertEqual(plan.delete_packages, (
            "kmahjongg", "kpat", "ksudoku", "katawa-shoujo",
            "plasma-vault", "krdc", "neochat", "konversation", "skanlite", "akregator", "dragonplayer", "gimp",
            "juk", "kdeconnect", "kmail", "kmouth", "konqueror", "korganizer", "kwrite", "anydesk",
        ))
        steps = plan_execution_steps(plan.packages, plan.profile_scripts, PackageManager.APT, plan.delete_packages)
        self.assertEqual(steps[-1].packages, plan.delete_packages)
        self.assertEqual(steps[-1].commands[0].argv[:2], ("bash", "-c"))

    def test_catalog_rejects_unknown_resource_fields(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            package_path = root / "invalid-package.toml"
            package_path.write_text(
                "[package]\nname = 'invalid'\nunexpected = true\n\n[targets.linux.apt]\nid = 'invalid'\n",
                encoding="utf-8",
            )
            insecure_installer_path = root / "insecure-installer.toml"
            insecure_installer_path.write_text(
                "[package]\nname = 'insecure'\n\n[targets.linux.shell_installer]\nid = 'http://example.com/install.sh'\n",
                encoding="utf-8",
            )
            profile_path = root / "invalid-profile.toml"
            profile_path.write_text(
                "[profile]\nname = 'invalid'\nrequired_packages = []\noptional_packages = []\n\n[platforms.linix]\nrequired_packages = []\noptional_packages = []\n",
                encoding="utf-8",
            )
            with self.assertRaisesRegex(ValueError, "unsupported fields: unexpected"):
                load_package(package_path)
            with self.assertRaisesRegex(ValueError, "unsupported platform 'linix'"):
                load_profile(profile_path)
            with self.assertRaisesRegex(ValueError, "must be an HTTPS URL"):
                load_package(insecure_installer_path)

    def test_unknown_profile_is_rejected(self):
        with self.assertRaises(PackageResolutionError):
            resolve_profiles(["does-not-exist"], self.catalog, self.profiles, "linux", ("apt",))


if __name__ == "__main__":
    unittest.main()
