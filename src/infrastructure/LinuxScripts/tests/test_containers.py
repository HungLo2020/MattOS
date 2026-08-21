"""Regression coverage for the Python container-management migration."""

from __future__ import annotations

import importlib.util
import sys
import unittest
from pathlib import Path
from unittest.mock import patch


ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "src"))

from containers.workloads import Action, UptimeKumaWorkload, WORKLOADS, StableDiffusionWorkload, parse_action


def load_tool(name: str):
    """Load a Tool module without running its script entry point."""

    path = ROOT / "Tools" / name
    specification = importlib.util.spec_from_file_location(path.stem, path)
    module = importlib.util.module_from_spec(specification)
    assert specification.loader is not None
    specification.loader.exec_module(module)
    return module


class ContainerMigrationTests(unittest.TestCase):
    def test_legacy_workload_actions_are_preserved(self):
        self.assertIs(parse_action([]), Action.RUN)
        self.assertIs(parse_action(["--on"]), Action.ON)
        self.assertIs(parse_action(["--off"]), Action.OFF)
        self.assertIs(parse_action(["-D"]), Action.DELETE)
        with self.assertRaisesRegex(ValueError, "unknown argument"):
            parse_action(["--delete"])

    def test_all_legacy_workloads_have_python_implementations(self):
        self.assertEqual(tuple(WORKLOADS), ("homepage", "jellyfin", "ollama", "portainer", "stable-diffusion"))
        for path in (
            ROOT / "resources" / "homepage" / "settings.yaml",
            ROOT / "resources" / "jellyfin" / "docker-compose.yml",
            ROOT / "resources" / "jellyfin" / ".env.example",
        ):
            self.assertTrue(path.is_file(), path)

    def test_stable_diffusion_image_keeps_legacy_version_label(self):
        dockerfile = StableDiffusionWorkload().dockerfile()
        self.assertIn('LABEL version="3"', dockerfile)
        self.assertIn("stable-diffusion-webui", dockerfile)

    def test_container_manager_queues_legacy_install_action(self):
        manager = load_tool("ContainerManager.py")
        with patch("builtins.input", side_effect=["-I", "--skip", "--skip", "--skip", "--end"]):
            queued = manager.choose_actions()
        self.assertEqual(queued, [("homepage", Action.RUN)])

    def test_server_manager_exposes_container_manager(self):
        manager = load_tool("ServerManager.py")
        names = [name for name, _, _ in manager.capabilities()]
        self.assertIn("Container manager", names)
        self.assertIn("Btrfs snapshot manager", names)
        self.assertIn("Restic backup manager", names)
        self.assertIn("ZIP backup manager", names)
        self.assertIn("Uptime Kuma", names)

    def test_server_manager_launches_container_manager_as_current_user(self):
        manager = load_tool("ServerManager.py")
        with patch.object(manager.subprocess, "run") as run_command:
            run_command.return_value.returncode = 0
            self.assertEqual(manager.container_manager_action(), 0)
        self.assertEqual(run_command.call_args.args[0][0], manager.sys.executable)

    def test_uptime_kuma_uses_legacy_container_paths_and_arguments(self):
        workload = UptimeKumaWorkload()
        with patch("containers.workloads.Docker") as docker_type, patch("containers.workloads.wait_for_http", return_value=True):
            docker = docker_type.return_value
            docker.ensure_available.return_value = True
            docker.container_exists.return_value = False
            docker.container_running.return_value = False
            with patch.object(workload, "port_in_use", return_value=False):
                workload.execute(Action.RUN)
        self.assertEqual(workload.data_directory, Path.home() / ".uptime-kuma" / "data")
        self.assertIn((("pull", "louislam/uptime-kuma:latest"),), docker.run.call_args_list)
        self.assertIn(
            (("run", "-d", "--name", "uptime-kuma", "--restart", "unless-stopped", "-p", "3002:3001", "-v", f"{workload.data_directory}:/app/data", "louislam/uptime-kuma:latest"),),
            docker.run.call_args_list,
        )


if __name__ == "__main__":
    unittest.main()