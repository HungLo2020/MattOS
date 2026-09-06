#!/usr/bin/env python3
import sys
import unittest
from pathlib import Path
from tempfile import TemporaryDirectory
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import PublishPackages
from common import RepoError


class PublishPackagesTests(unittest.TestCase):
    def test_discovery_uses_inventory_and_ignores_stale_debs(self) -> None:
        with TemporaryDirectory() as temporary:
            root = Path(temporary)
            package_root = root / "out/packages/amd64"
            package_root.mkdir(parents=True)
            (package_root / "z.deb").touch()
            (package_root / "stale.deb").touch()
            (root / "out/packages/inventory.toml").write_text(
                '[[package]]\nartifact_path = "out/packages/amd64/z.deb"\n',
                encoding="utf-8",
            )
            self.assertEqual(
                [path.relative_to(root).as_posix() for path in PublishPackages.discover_packages(root)],
                ["out/packages/amd64/z.deb"],
            )

    def test_discovery_rejects_symlinked_packages(self) -> None:
        with TemporaryDirectory() as temporary:
            root = Path(temporary)
            package_root = root / "out/packages/amd64"
            package_root.mkdir(parents=True)
            outside = root / "outside.deb"
            outside.touch()
            (package_root / "escape.deb").symlink_to(outside)
            (root / "out/packages/inventory.toml").write_text(
                '[[package]]\nartifact_path = "out/packages/amd64/escape.deb"\n',
                encoding="utf-8",
            )
            with self.assertRaises(RepoError):
                PublishPackages.discover_packages(root)

    def test_build_reuses_run_qemu_build_helper(self) -> None:
        with mock.patch("PublishPackages.run_qemu.build_if_needed") as build:
            PublishPackages.ensure_build(Path("/repo"), clean=True, no_build=False)
            args = build.call_args.args
            self.assertEqual(args[0], Path("/repo"))
            self.assertFalse(args[1].no_build)
            self.assertTrue(args[1].clean)

    def test_upload_delegates_all_discovered_packages_to_vendored_manager(self) -> None:
        packages = [Path("/repo/out/packages/amd64/a.deb"), Path("/repo/out/packages/amd64/b.deb")]
        with mock.patch("PublishPackages.run_command") as run:
            with mock.patch.object(PublishPackages, "publisher_path", return_value=Path("/repo/ManageMattOSRepository.py")):
                PublishPackages.upload_packages(Path("/repo"), packages, dry_run=True)
        command = run.call_args.args[0]
        self.assertEqual(command[:4], [sys.executable, "/repo/ManageMattOSRepository.py", "--non-interactive", "--dry-run"])
        self.assertEqual(command[4:], ["--repo", "mattos", "upload", *(str(path) for path in packages)])

    def test_upload_selects_mattos_without_dry_run(self) -> None:
        packages = [Path("/repo/out/packages/amd64/a.deb")]
        with mock.patch("PublishPackages.run_command") as run:
            with mock.patch.object(PublishPackages, "publisher_path", return_value=Path("/repo/ManageMattOSRepository.py")):
                PublishPackages.upload_packages(Path("/repo"), packages, dry_run=False)
        self.assertEqual(
            run.call_args.args[0],
            [sys.executable, "/repo/ManageMattOSRepository.py", "--non-interactive", "--repo", "mattos", "upload", str(packages[0])],
        )


if __name__ == "__main__":
    unittest.main()
