#!/usr/bin/env python3
import hashlib
import io
import sys
import tarfile
import unittest
from pathlib import Path
from tempfile import TemporaryDirectory

from compression import zstd

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import run_qemu


def synthetic_deb(version: str) -> bytes:
    def tar(files: dict[str, bytes]) -> bytes:
        buffer = io.BytesIO()
        with tarfile.open(fileobj=buffer, mode="w") as archive:
            for name, body in files.items():
                info = tarfile.TarInfo(name)
                info.size = len(body)
                archive.addfile(info, io.BytesIO(body))
        return zstd.compress(buffer.getvalue())

    control = f"Package: kcalc\nVersion: {version}\nArchitecture: amd64\nDescription: test\n".encode()
    return run_qemu._ar_archive([
        ("debian-binary", b"2.0\n"),
        ("control.tar.zst", tar({"./control": control})),
        ("data.tar.zst", tar({"./usr/bin/kcalc": b"binary"})),
    ])


class UpdateProbeRepositoryTests(unittest.TestCase):
    def test_rebuild_changes_only_the_version_and_keeps_the_payload(self) -> None:
        original = synthetic_deb("26.08.1-1mattos2")
        rebuilt, control = run_qemu.rebuild_deb_with_version(original, "26.08.1-1mattos2+updatetest1")
        self.assertIn("Version: 26.08.1-1mattos2+updatetest1\n", control)
        self.assertIn("Package: kcalc\n", control)
        before = dict(run_qemu._ar_members(original))
        after = dict(run_qemu._ar_members(rebuilt))
        self.assertEqual(list(after), ["debian-binary", "control.tar.zst", "data.tar.zst"])
        self.assertEqual(after["data.tar.zst"], before["data.tar.zst"])
        self.assertEqual(after["debian-binary"], b"2.0\n")

    def test_flat_repository_indexes_the_bumped_package(self) -> None:
        with TemporaryDirectory() as temporary:
            repo = Path(temporary)
            artifact = repo / "out/packages/amd64/kcalc_26.08.1-1mattos2_amd64.deb"
            artifact.parent.mkdir(parents=True)
            artifact.write_bytes(synthetic_deb("26.08.1-1mattos2"))
            (repo / "out/packages/inventory.toml").write_text(
                '[[package]]\nname = "kcalc"\nversion = "26.08.1-1mattos2"\narchitecture = "amd64"\n'
                f'artifact_path = "{artifact.relative_to(repo)}"\n',
                encoding="utf-8",
            )
            destination = repo / "out/tmp/update-probe-repository"
            package, version = run_qemu.prepare_update_probe_repository(repo, destination)
            self.assertEqual((package, version), ("kcalc", "26.08.1-1mattos2+updatetest1"))
            deb = destination / "kcalc_26.08.1-1mattos2+updatetest1_amd64.deb"
            packages = (destination / "Packages").read_bytes()
            self.assertIn(f"SHA256: {hashlib.sha256(deb.read_bytes()).hexdigest()}".encode(), packages)
            self.assertIn(b"Filename: ./kcalc_26.08.1-1mattos2+updatetest1_amd64.deb", packages)
            self.assertIn(hashlib.sha256(packages).hexdigest(), (destination / "Release").read_text())

    def test_probes_cover_packagekit_the_notifier_and_cleanup(self) -> None:
        probes = dict(run_qemu.installed_discover_update_probes(41234, "kcalc", "1-1mattos2+updatetest1"))
        self.assertEqual(
            list(probes),
            [
                "update-probe-source",
                "discover-notifier-running",
                "packagekit-refresh",
                "packagekit-offers-update",
                "discover-notifier-offers-update",
                "packagekit-installs-update",
                "update-probe-cleanup",
            ],
        )
        self.assertIn("http://10.0.2.2:41234/ ./", probes["update-probe-source"])
        self.assertIn("pkgcli -y refresh", probes["packagekit-refresh"])
        self.assertIn("1-1mattos2+updatetest1", probes["packagekit-offers-update"])
        self.assertIn("DiscoverNotifier", probes["discover-notifier-offers-update"])
        self.assertIn("Active", probes["discover-notifier-offers-update"])
        self.assertIn("pkgcli -y update kcalc", probes["packagekit-installs-update"])
        self.assertIn("rm -f /etc/apt/sources.list.d/zz-mattos-update-probe.list", probes["update-probe-cleanup"])
        # The UART transport needs each probe short and on one line.
        self.assertTrue(all("\n" not in command and len(command) < 800 for command in probes.values()))


if __name__ == "__main__":
    unittest.main()
