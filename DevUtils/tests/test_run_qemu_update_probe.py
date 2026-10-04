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


def synthetic_deb(version: str, package: str = "kcalc", depends: str = "") -> bytes:
    def tar(files: dict[str, bytes]) -> bytes:
        buffer = io.BytesIO()
        with tarfile.open(fileobj=buffer, mode="w") as archive:
            for name, body in files.items():
                info = tarfile.TarInfo(name)
                info.size = len(body)
                archive.addfile(info, io.BytesIO(body))
        return zstd.compress(buffer.getvalue())

    depends_line = f"Depends: {depends}\n" if depends else ""
    control = f"Package: {package}\nVersion: {version}\nArchitecture: amd64\n{depends_line}Description: test\n".encode()
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
            updates = run_qemu.prepare_update_probe_repository(repo, destination)
            self.assertEqual(updates, {"kcalc": "26.08.1-1mattos2+updatetest1"})
            deb = destination / "kcalc_26.08.1-1mattos2+updatetest1_amd64.deb"
            packages = (destination / "Packages").read_bytes()
            self.assertIn(f"SHA256: {hashlib.sha256(deb.read_bytes()).hexdigest()}".encode(), packages)
            self.assertIn(b"Filename: ./kcalc_26.08.1-1mattos2+updatetest1_amd64.deb", packages)
            self.assertIn(hashlib.sha256(packages).hexdigest(), (destination / "Release").read_text())

    def test_exact_dependents_are_repinned_like_a_publish(self) -> None:
        # A profile metapackage pins its applications exactly; publishing
        # raises its revision with theirs, so the probe must offer it too or
        # updating the application alone would remove the metapackage.
        with TemporaryDirectory() as temporary:
            repo = Path(temporary)
            amd64 = repo / "out/packages/amd64"
            amd64.mkdir(parents=True)
            debs = {
                "kcalc": ("26.08.1-1mattos2", "libc6 (= 2.43-1mattos1)"),
                "mattos-plasma": ("0.1-1mattos4", "libc6 (= 2.43-1mattos1), kcalc (= 26.08.1-1mattos2), kcalc-extra"),
                "mattos-plasma-live": ("0.1-1mattos1", "mattos-plasma (= 0.1-1mattos4)"),
                "unrelated": ("1-1mattos1", "kcalc (= 1-1mattos1)"),
            }
            inventory = ""
            for name, (version, depends) in debs.items():
                artifact = amd64 / f"{name}_{version}_amd64.deb"
                artifact.write_bytes(synthetic_deb(version, name, depends))
                relations = ", ".join(f'"{relation.strip()}"' for relation in depends.split(","))
                inventory += (
                    f'[[package]]\nname = "{name}"\nversion = "{version}"\narchitecture = "amd64"\n'
                    f'artifact_path = "{artifact.relative_to(repo)}"\ndependencies = [{relations}]\n\n'
                )
            (repo / "out/packages/inventory.toml").write_text(inventory, encoding="utf-8")
            destination = repo / "out/tmp/update-probe-repository"
            updates = run_qemu.prepare_update_probe_repository(repo, destination)
            self.assertEqual(
                updates,
                {
                    "kcalc": "26.08.1-1mattos2+updatetest1",
                    "mattos-plasma": "0.1-1mattos4+updatetest1",
                    "mattos-plasma-live": "0.1-1mattos1+updatetest1",
                },
            )
            packages = (destination / "Packages").read_text()
            self.assertIn(
                "Depends: libc6 (= 2.43-1mattos1), kcalc (= 26.08.1-1mattos2+updatetest1), kcalc-extra\n",
                packages,
            )
            self.assertIn("Depends: mattos-plasma (= 0.1-1mattos4+updatetest1)\n", packages)
            self.assertNotIn("unrelated", packages)

    def test_probes_cover_packagekit_the_notifier_and_cleanup(self) -> None:
        probes = dict(run_qemu.installed_discover_update_probes(
            41234, {"kcalc": "1-1mattos2+updatetest1", "mattos-plasma": "0.1-1mattos4+updatetest1"}
        ))
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
        # The installed MattOS repositories are pinned at 990; at a lower
        # priority the probe's newer build never becomes APT's candidate.
        self.assertIn("Pin: origin 10.0.2.2", probes["update-probe-source"])
        self.assertIn("Pin-Priority: 990", probes["update-probe-source"])
        self.assertIn("/etc/apt/preferences.d/zz-mattos-update-probe", probes["update-probe-source"])
        self.assertIn("pkgcli -y refresh", probes["packagekit-refresh"])
        self.assertIn("1-1mattos2+updatetest1", probes["packagekit-offers-update"])
        # Nothing may be removed to take the update.
        self.assertIn("! pkgcli --json list-updates", probes["packagekit-offers-update"])
        self.assertIn('"state": ?"remove"', probes["packagekit-offers-update"])
        # Upstream hides the status-notifier item right after announcing
        # normal updates, so the probe waits for the announcement record from
        # a restarted notifier with cleared state.
        self.assertIn("systemctl --user start app-org.kde.discover.notifier@autostart.service", probes["discover-notifier-offers-update"])
        self.assertIn("rm -f ~/.local/state/discovernotifierstaterc", probes["discover-notifier-offers-update"])
        self.assertIn("grep -q LastNotificationTime", probes["discover-notifier-offers-update"])
        self.assertIn("pkgcli -y update kcalc mattos-plasma", probes["packagekit-installs-update"])
        self.assertIn("mattos-plasma)\" = '0.1-1mattos4+updatetest1'", probes["packagekit-installs-update"])
        self.assertIn("rm -f /etc/apt/sources.list.d/zz-mattos-update-probe.list", probes["update-probe-cleanup"])
        self.assertIn("/etc/apt/preferences.d/zz-mattos-update-probe", probes["update-probe-cleanup"])
        # The UART transport needs each probe short and on one line.
        self.assertTrue(all("\n" not in command and len(command) < 800 for command in probes.values()))


if __name__ == "__main__":
    unittest.main()
