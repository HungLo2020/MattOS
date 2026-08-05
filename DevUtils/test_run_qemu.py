#!/usr/bin/env python3
import os
import subprocess
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from run_qemu import image_build_commands, network_arguments


class QemuNetworkArgumentsTests(unittest.TestCase):
    def test_default_network_is_unprivileged_virtio(self) -> None:
        self.assertEqual(
            network_arguments(False),
            ["-netdev", "user,id=net0", "-device", "virtio-net-pci,netdev=net0"],
        )

    def test_no_network_omits_all_network_arguments(self) -> None:
        self.assertEqual(network_arguments(True), [])

    def test_launcher_invokes_one_image_producing_build(self) -> None:
        commands = image_build_commands(False)
        self.assertEqual(
            commands,
            [["cargo", "run", "-p", "mattos-build", "--", "build", "all"]],
        )
        self.assertNotIn("image", [argument for command in commands for argument in command])

    def test_clean_build_still_has_one_image_producing_build(self) -> None:
        commands = image_build_commands(True)
        self.assertEqual(len(commands), 2)
        self.assertEqual(commands[-1][-2:], ["build", "all"])


@unittest.skipUnless(
    os.environ.get("MATTOS_RUN_FRESH_PROCESS_CACHE_TESTS") == "1",
    "set MATTOS_RUN_FRESH_PROCESS_CACHE_TESTS=1 for the full cache integration test",
)
class FreshProcessCacheIntegrationTests(unittest.TestCase):
    FOUNDATIONAL_STAGES = (
        "linux",
        "glibc",
        "linux-headers",
        "gcc-runtime",
        "binutils",
        "gcc-compiler",
        "make",
        "formal-sysroot",
    )

    def run_fresh(self, command: list[str], **environment: str) -> str:
        repo_root = Path(__file__).resolve().parents[1]
        child_environment = os.environ.copy()
        child_environment.update(environment)
        completed = subprocess.run(
            command,
            cwd=repo_root,
            env=child_environment,
            check=False,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
        )
        self.assertEqual(completed.returncode, 0, completed.stdout)
        return completed.stdout

    def assert_foundational_hits(self, output: str) -> None:
        for stage in self.FOUNDATIONAL_STAGES:
            self.assertIn(f"cache hit: {stage} ", output, output)
            self.assertNotIn(f"cache miss: {stage} ", output, output)

    def test_direct_launcher_and_cross_path_cache_stability(self) -> None:
        direct = ["cargo", "run", "-p", "mattos-build", "--", "build", "all"]
        launcher = [sys.executable, "DevUtils/run_qemu.py", "--build-only"]

        # The first direct process may perform the one-time schema migration.
        self.run_fresh(direct, TERM="dumb", COLUMNS="80", LINES="24")
        launcher_one = self.run_fresh(
            launcher,
            TERM="xterm-256color",
            COLORTERM="truecolor",
            COLUMNS="240",
            LINES="60",
            MATTOS_VERBOSE_BUILD_OUTPUT="",
            QEMU_AUDIO_DRV="none",
        )
        launcher_two = self.run_fresh(
            launcher,
            TERM="screen-256color",
            COLORTERM="24bit",
            COLUMNS="132",
            LINES="43",
            QEMU_AUDIO_DRV="pa",
        )
        direct_two = self.run_fresh(direct, TERM="dumb", COLUMNS="72", LINES="20")

        self.assert_foundational_hits(launcher_one)
        self.assert_foundational_hits(launcher_two)
        self.assert_foundational_hits(direct_two)


if __name__ == "__main__":
    unittest.main()
