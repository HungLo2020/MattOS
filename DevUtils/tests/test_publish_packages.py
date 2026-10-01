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
        with mock.patch("PublishPackages.subprocess.run", return_value=mock.Mock(returncode=0, stdout="")) as run:
            with mock.patch.object(PublishPackages, "publisher_path", return_value=Path("/repo/ManageMattOSRepository.py")):
                PublishPackages.upload_packages(Path("/repo"), packages, dry_run=True)
        command = run.call_args.args[0]
        self.assertEqual(command[:4], [sys.executable, "/repo/ManageMattOSRepository.py", "--non-interactive", "--dry-run"])
        self.assertEqual(command[4:], ["--repo", "mattos", "upload", *(str(path) for path in packages)])

    def test_upload_selects_mattos_without_dry_run(self) -> None:
        packages = [Path("/repo/out/packages/amd64/a.deb")]
        with mock.patch("PublishPackages.subprocess.run", return_value=mock.Mock(returncode=0, stdout="")) as run:
            with mock.patch.object(PublishPackages, "publisher_path", return_value=Path("/repo/ManageMattOSRepository.py")):
                PublishPackages.upload_packages(Path("/repo"), packages, dry_run=False)
        self.assertEqual(
            run.call_args.args[0],
            [sys.executable, "/repo/ManageMattOSRepository.py", "--non-interactive", "--repo", "mattos", "upload", str(packages[0])],
        )


    def test_transient_upload_failures_are_retried_ten_times_ten_seconds_apart(self) -> None:
        packages = [Path("/repo/out/packages/amd64/a.deb")]
        unreachable = mock.Mock(returncode=40, stdout="Error [remote]: Repository server is unreachable: http://x\n")
        rejected = mock.Mock(returncode=40, stdout="Error [remote]: Repository server returned HTTP 400: bad\n")
        ok = mock.Mock(returncode=0, stdout="uploaded\n")
        patches = (mock.patch.object(PublishPackages, "publisher_path", return_value=Path("/repo/m.py")),
                   mock.patch("PublishPackages.time.sleep"))
        with patches[0], patches[1] as sleep, \
             mock.patch("PublishPackages.subprocess.run", side_effect=[unreachable] * 10 + [ok]) as run:
            PublishPackages.upload_packages(Path("/repo"), packages, dry_run=False)
        self.assertEqual(run.call_count, 11)
        self.assertEqual([call.args[0] for call in sleep.call_args_list], [10] * 10)
        with patches[0], patches[1], \
             mock.patch("PublishPackages.subprocess.run", return_value=unreachable) as run, \
             self.assertRaisesRegex(PublishPackages.RepoError, "gave up after 10 retries"):
            PublishPackages.upload_packages(Path("/repo"), packages, dry_run=False)
        self.assertEqual(run.call_count, 11)
        with patches[0], patches[1] as sleep, \
             mock.patch("PublishPackages.subprocess.run", return_value=rejected) as run, \
             self.assertRaisesRegex(PublishPackages.RepoError, "HTTP 400"):
            PublishPackages.upload_packages(Path("/repo"), packages, dry_run=False)
        self.assertEqual((run.call_count, sleep.call_count), (1, 0))

    def test_index_fetch_retries_transient_failures_only(self) -> None:
        import gzip
        import urllib.error

        class Response:
            def __enter__(self):
                return self

            def __exit__(self, *_args):
                return False

            def read(self):
                return gzip.compress(b"Package: a\n")

        def http(code):
            return urllib.error.HTTPError("u", code, "x", {}, None)

        with mock.patch("PublishPackages.time.sleep") as sleep:
            with mock.patch("PublishPackages.urllib.request.urlopen",
                            side_effect=[OSError("refused"), http(503), Response()]):
                self.assertEqual(PublishPackages.fetch_published_index("https://x"), "Package: a\n")
            self.assertEqual(sleep.call_count, 2)
            sleep.reset_mock()
            with mock.patch("PublishPackages.urllib.request.urlopen", side_effect=http(404)), \
                 self.assertRaises(PublishPackages.RepoError):
                PublishPackages.fetch_published_index("https://x")
            self.assertEqual(sleep.call_count, 0)


def _package(name: str, version: str, sha: str = "a" * 64, dependencies: tuple[str, ...] = ()):
    return PublishPackages.InventoryPackage(
        name=name,
        version=version,
        architecture="amd64",
        sha256=sha,
        artifact=Path(f"/repo/out/packages/amd64/{name}_{version}_amd64.deb"),
        dependencies=dependencies,
    )


class PublicationGuardTests(unittest.TestCase):
    INDEX = (
        "Package: libc6\nVersion: 2.43-1mattos1\nArchitecture: amd64\nSHA256: " + "a" * 64 + "\n"
        "Description: C library\n continued description line: not a field\n\n"
        "Package: bash\nVersion: 5.3-1mattos1\nArchitecture: amd64\nSHA256: " + "b" * 64 + "\n\n"
        "Package: linux-libc-dev\nVersion: 0~git.f17f39c917cd-1mattos1\nArchitecture: amd64\nSHA256: "
        + "c" * 64 + "\n"
    )

    def test_index_parsing_keeps_versions_and_hashes(self) -> None:
        index = PublishPackages.parse_packages_index(self.INDEX)
        self.assertEqual(
            index[("libc6", "amd64")], [PublishPackages.PublishedPackage("2.43-1mattos1", "a" * 64)]
        )
        self.assertEqual(len(index), 3)

    def test_plan_skips_identical_replaces_rebuilt_and_refuses_older(self) -> None:
        published = PublishPackages.parse_packages_index(self.INDEX)
        plan = PublishPackages.plan_publication(
            [
                _package("libc6", "2.43-1mattos1"),  # identical: skipped
                _package("bash", "5.3-1mattos1", sha="d" * 64),  # rebuilt: replaced
                _package("linux-libc-dev", "0~git.8ba098e6b6ff-1mattos1"),  # sorts lower
                _package("zlib1g", "1.3.2-1mattos1"),  # not published yet
                _package("bash-doc", "5.4-1mattos1"),  # not published yet
            ],
            published,
        )
        self.assertEqual([p.name for p in plan.unchanged], ["libc6"])
        self.assertEqual([p.name for p in plan.rebuilt], ["bash"])
        self.assertEqual(
            [(p.name, newest) for p, newest in plan.older_than_published],
            [("linux-libc-dev", "0~git.f17f39c917cd-1mattos1")],
        )
        self.assertEqual([p.name for p in plan.new], ["zlib1g", "bash-doc"])
        self.assertEqual([p.name for p in plan.upload], ["zlib1g", "bash-doc", "bash"])

    def test_a_newer_version_is_uploaded(self) -> None:
        published = PublishPackages.parse_packages_index(self.INDEX)
        plan = PublishPackages.plan_publication([_package("bash", "5.4-1mattos1", sha="d" * 64)], published)
        self.assertEqual([p.name for p in plan.new], ["bash"])
        self.assertEqual(plan.rebuilt, [])

    def test_a_snapshot_version_upgrades_the_retired_hash_form(self) -> None:
        published = PublishPackages.parse_packages_index(self.INDEX)
        plan = PublishPackages.plan_publication(
            [_package("linux-libc-dev", "7.2.0~rc5+git20260801.101500.8ba098e6b6ff-1mattos1")], published
        )
        self.assertEqual(plan.older_than_published, [])
        self.assertEqual(len(plan.upload), 1)

    def test_main_uploads_only_new_and_rebuilt_packages(self) -> None:
        packages = [
            _package("libc6", "2.43-1mattos1"),
            _package("bash", "5.3-1mattos1", sha="d" * 64),
            _package("zlib1g", "1.3.2-1mattos1"),
        ]
        with (
            mock.patch.object(PublishPackages, "parse_args", return_value=mock.Mock(clean=False, no_build=True, dry_run=True)),
            mock.patch.object(PublishPackages, "find_repo_root", return_value=Path("/repo")),
            mock.patch.object(PublishPackages, "discover_packages", return_value=[]),
            mock.patch.object(PublishPackages, "inventory_packages", return_value=packages),
            mock.patch.object(PublishPackages, "third_party_package_names", return_value=set()),
            mock.patch.object(PublishPackages, "published_index_url", return_value="https://x/Packages.gz"),
            mock.patch.object(PublishPackages, "fetch_published_index", return_value=self.INDEX),
            mock.patch.object(PublishPackages, "upload_packages") as upload,
        ):
            self.assertEqual(PublishPackages.main(), 0)
        uploaded = upload.call_args.args[1]
        self.assertEqual(sorted(path.name for path in uploaded), ["bash_5.3-1mattos1_amd64.deb", "zlib1g_1.3.2-1mattos1_amd64.deb"])
        self.assertTrue(upload.call_args.kwargs["dry_run"])

    def test_main_refuses_older_versions_without_uploading(self) -> None:
        with (
            mock.patch.object(PublishPackages, "parse_args", return_value=mock.Mock(clean=False, no_build=True, dry_run=False)),
            mock.patch.object(PublishPackages, "find_repo_root", return_value=Path("/repo")),
            mock.patch.object(PublishPackages, "discover_packages", return_value=[]),
            mock.patch.object(
                PublishPackages, "inventory_packages", return_value=[_package("linux-libc-dev", "0~git.0-1mattos1")]
            ),
            mock.patch.object(PublishPackages, "published_index_url", return_value="https://x/Packages.gz"),
            mock.patch.object(PublishPackages, "fetch_published_index", return_value=self.INDEX),
            mock.patch.object(PublishPackages, "upload_packages") as upload,
        ):
            with self.assertRaises(RepoError):
                PublishPackages.main()
        upload.assert_not_called()

    def test_names_claimed_by_third_party_recipes_are_refused(self) -> None:
        with TemporaryDirectory() as temporary:
            root = Path(temporary)
            releases = root / PublishPackages.THIRD_PARTY_RELEASES_RELATIVE
            releases.parent.mkdir(parents=True)
            releases.write_text('{"format": 1, "packages": {"btop": {"version": "1"}}}', encoding="utf-8")
            PublishPackages.reject_third_party_names(root, [_package("libc6", "2.43-1mattos1")])
            with self.assertRaises(RepoError):
                PublishPackages.reject_third_party_names(root, [_package("btop", "1.4.7-1mattos1")])

    def test_index_url_comes_from_the_installed_hosted_source(self) -> None:
        with TemporaryDirectory() as temporary:
            root = Path(temporary)
            sources = root / PublishPackages.REPOSITORY_SOURCES_RELATIVE
            sources.parent.mkdir(parents=True)
            sources.write_text(
                "Types: deb\nURIs: https://packages.example.com/\nSuites: trixie\n"
                "Components: main\nArchitectures: amd64\n",
                encoding="utf-8",
            )
            self.assertEqual(
                PublishPackages.published_index_url(root),
                "https://packages.example.com/dists/trixie/main/binary-amd64/Packages.gz",
            )


if __name__ == "__main__":
    unittest.main()
