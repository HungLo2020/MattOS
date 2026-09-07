from __future__ import annotations

import sys
import tempfile
import unittest
import runpy
import json
from pathlib import Path
from unittest import mock


ROOT = Path(__file__).resolve().parents[2]
PACKAGE_ROOT = ROOT / "third-party-packages"
sys.path.insert(0, str(PACKAGE_ROOT))
import common.framework as framework
import common


class FrameworkTests(unittest.TestCase):
    def test_common_exports_match_framework_symbols(self):
        self.assertEqual(len(common.__all__), len(set(common.__all__)))
        for name in common.__all__:
            self.assertTrue(hasattr(common, name), name)
            self.assertTrue(hasattr(framework, name), name)

    def load_recipe(self, filename: str):
        return runpy.run_path(str(PACKAGE_ROOT / filename), run_name=f"test_{filename}")

    def test_framework_json_helpers_and_version_parsing(self):
        class Response:
            def __init__(self, body):
                self.body = body

            def __enter__(self):
                return self

            def __exit__(self, *_args):
                return False

            def read(self):
                return self.body

        response = Response(json.dumps({"tag_name": "v2.68.1"}).encode())
        with mock.patch.object(framework, "urlopen", return_value=response):
            version, provenance = framework.github_latest_release("fastfetch-cli", "fastfetch")
        self.assertEqual(version, "2.68.1")
        self.assertEqual(provenance["release_tag"], "v2.68.1")

        bad_response = Response(b"[]")
        with mock.patch.object(framework, "urlopen", return_value=bad_response):
            with self.assertRaises(framework.RecipeError):
                framework.github_latest_release("owner", "missing")

    def test_real_recipe_modules_have_required_contract_metadata(self):
        for filename in ("fastfetch.py", "firefox.py"):
            with self.subTest(filename=filename):
                namespace = self.load_recipe(filename)
                recipe_types = [value for value in namespace.values()
                                if isinstance(value, type) and issubclass(value, framework.PackageRecipe)
                                and value is not framework.PackageRecipe]
                self.assertEqual(len(recipe_types), 1)
                recipe = recipe_types[0]()
                self.assertTrue(recipe.name)
                self.assertEqual(framework.validate_repository(recipe), "mattos")
                self.assertTrue(recipe.description)
                self.assertEqual(recipe.architecture, "amd64")
                self.assertTrue(recipe.dependency_names())

    def test_real_recipe_no_argument_entrypoints_are_read_only_checks(self):
        for filename, response in (
            ("fastfetch.py", {"tag_name": "v2.68.1"}),
            ("firefox.py", {"LATEST_FIREFOX_VERSION": "155.0"}),
        ):
            with self.subTest(filename=filename):
                module = self.load_recipe(filename)
                recipe_type = next(value for value in module.values()
                                   if isinstance(value, type) and issubclass(value, framework.PackageRecipe)
                                   and value is not framework.PackageRecipe)
                with mock.patch.object(framework, "repo_root", return_value=ROOT), \
                     mock.patch.object(framework, "repository_versions", return_value=[]), \
                     mock.patch.object(framework, "github_latest_release", return_value=("2.68.1", {"release_tag": "v2.68.1"})), \
                     mock.patch.object(common, "github_latest_release", return_value=("2.68.1", {"release_tag": "v2.68.1"})), \
                     mock.patch.object(common, "fetch_json", return_value=response):
                    with mock.patch.object(sys, "argv", [str(PACKAGE_ROOT / filename)]):
                        with self.assertRaises(SystemExit) as exit_info:
                            runpy.run_path(str(PACKAGE_ROOT / filename), run_name="__main__")
                self.assertEqual(exit_info.exception.code, 0)

    def test_shipped_recipes_declare_mattos(self):
        fastfetch = runpy.run_path(str(PACKAGE_ROOT / "fastfetch.py"))["FastfetchRecipe"]
        firefox = runpy.run_path(str(PACKAGE_ROOT / "firefox.py"))["FirefoxRecipe"]
        self.assertEqual(framework.validate_repository(fastfetch()), "mattos")
        self.assertEqual(framework.validate_repository(firefox()), "mattos")

    def test_repository_metadata_is_required_and_allowlist_is_exact(self):
        class Missing(framework.PackageRecipe):
            name = "missing"

        for value in ("", "debian", "mattos-extra"):
            recipe = type("Invalid", (framework.PackageRecipe,), {
                "name": "invalid", "repository": value,
            })()
            with self.subTest(value=value), self.assertRaises(framework.RecipeError):
                framework.validate_repository(recipe)
        with self.assertRaises(framework.RecipeError):
            framework.validate_repository(Missing())
        fixture = type("MattPackages", (framework.PackageRecipe,), {
            "name": "fixture", "repository": "mattpackages",
        })()
        self.assertEqual(framework.validate_repository(fixture), "mattpackages")

    def test_repository_listing_selects_declared_repository(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            publisher = root / "src/infrastructure/LinuxScripts/GenericScripts/ManageMattOSRepository.py"
            publisher.parent.mkdir(parents=True)
            publisher.touch()
            for repository in ("mattos", "mattpackages"):
                with self.subTest(repository=repository), mock.patch.object(
                    framework, "command", return_value="fastfetch\t2.68.1\tamd64\n"
                ) as run:
                    self.assertEqual(framework.repository_versions(root, "fastfetch", repository), ["2.68.1"])
                self.assertEqual(
                    run.call_args.args[0][2:],
                    ["--non-interactive", "--repo", repository, "list"],
                )

    def test_repository_lookup_failure_is_not_version_absent(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            publisher = root / "src/infrastructure/LinuxScripts/GenericScripts/ManageMattOSRepository.py"
            publisher.parent.mkdir(parents=True)
            publisher.touch()
            with mock.patch.object(framework, "command", side_effect=framework.RecipeError("offline")):
                with self.assertRaises(framework.RecipeError):
                    framework.repository_versions(root, "fastfetch", "mattos")

    def test_publish_selects_declared_repository_for_normal_and_dry_run(self):
        artifact = Path("/repo/fastfetch.deb")
        for repository in ("mattos", "mattpackages"):
            for dry_run in (False, True):
                with self.subTest(repository=repository, dry_run=dry_run), mock.patch.object(framework, "command") as run:
                    framework.publish(Path("/repo"), artifact, repository=repository, dry_run=dry_run)
                args = [sys.executable, str(Path("/repo/src/infrastructure/LinuxScripts/GenericScripts/ManageMattOSRepository.py")), "--non-interactive"]
                if dry_run:
                    args.append("--dry-run")
                args += ["--repo", repository, "upload", str(artifact)]
                self.assertEqual(run.call_args.args[0], args)

    def test_archive_traversal_is_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            archive = Path(directory) / "bad.tar"
            import tarfile
            with tarfile.open(archive, "w") as tar:
                source = Path(directory) / "payload"
                source.write_text("bad")
                tar.add(source, arcname="../escape")
            with self.assertRaises(framework.RecipeError):
                framework.extract_archive(archive, Path(directory) / "out")

    def test_control_and_deb_creation_are_native(self):
        framework.require_tools(["dpkg-deb"])
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            staging = root / "package"
            binary = staging / "usr/bin/example"
            binary.parent.mkdir(parents=True)
            binary.write_text("#!/bin/sh\necho ok\n")
            binary.chmod(0o755)
            framework.write_control(staging, name="example", version="1.2.3", description="Example", depends=["libc6"])
            framework.write_provenance(staging, "example", {"source": "fixture", "version": "1.2.3"})
            artifact = framework.package_staging(staging, root / "dist", name="example", version="1.2.3")
            self.assertTrue(artifact.is_file())
            metadata = framework.command(["dpkg-deb", "--show", "--showformat=${Package} ${Version} ${Architecture}\n", str(artifact)])
            self.assertEqual(metadata.strip(), "example 1.2.3 amd64")

    def test_provenance_is_package_scoped_and_cmake_fixture_packages(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / "source"
            source.mkdir()
            (source / "CMakeLists.txt").write_text(
                "cmake_minimum_required(VERSION 3.16)\n"
                "project(fixture C)\n"
                "add_executable(fixture main.c)\n"
                "install(TARGETS fixture DESTINATION bin)\n"
            )
            (source / "main.c").write_text("int main(void) { return 0; }\n")
            staging = root / "staging"
            framework.cmake_build_install(source, root / "build", staging)
            first = framework.PackageRecipe()
            first.name = "fixture-one"
            first.repository = "mattpackages"
            first.description = "Fixture one"
            first_result = framework.finalize_package(first, staging, root / "out", "1.0-1", {"source": "one"})
            self.assertTrue(first_result.artifact.is_file())
            self.assertTrue((staging / "usr/share/mattos/third-party/fixture-one/provenance.json").is_file())
            self.assertIn('"repository": "mattpackages"',
                          (staging / "usr/share/mattos/third-party/fixture-one/provenance.json").read_text())

    def test_local_build_does_not_query_repository_and_persists_only_deb(self):
        class FixtureRecipe(framework.PackageRecipe):
            name = "fixture"
            repository = "mattos"
            description = "Fixture"

            def discover_version(self):
                return "1.0-1", {"source": "fixture"}

            def build(self, workspace, version, provenance):
                staging = workspace / "package"
                (staging / "usr/bin").mkdir(parents=True)
                (staging / "usr/bin/fixture").write_text("fixture\n")
                return framework.finalize_package(self, staging, workspace, version, provenance)

        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "Cargo.toml").write_text("[workspace]\n")
            (root / "upstream/sources.toml").parent.mkdir(parents=True)
            (root / "upstream/sources.toml").write_text("sources = []\n")
            destination = root / "dist"
            with mock.patch.object(framework, "repo_root", return_value=root), \
                 mock.patch.object(framework, "repository_versions", side_effect=AssertionError("build queried repository")):
                self.assertEqual(framework.run_recipe(FixtureRecipe(), ["build", "--output", str(destination)], root / "recipe.py"), 0)
            self.assertTrue((destination / "fixture_1.0-1_amd64.deb").is_file())
            self.assertEqual(list((root / "out/tmp").iterdir()), [])

    def test_no_argument_invocation_is_read_only_check(self):
        class FixtureRecipe(framework.PackageRecipe):
            name = "fixture"
            repository = "mattos"

            def discover_version(self):
                return "1.0", {"source": "fixture"}

            def build(self, workspace, version, provenance):
                raise AssertionError("check must not build")

        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            with mock.patch.object(framework, "repo_root", return_value=root), \
                 mock.patch.object(framework, "repository_versions", return_value=[]):
                self.assertEqual(framework.run_recipe(FixtureRecipe(), [], root / "recipe.py"), 0)

    def test_failed_update_cleans_ephemeral_workspace(self):
        class FailingRecipe(framework.PackageRecipe):
            name = "fixture"
            repository = "mattos"

            def discover_version(self):
                return "1.0", {"source": "fixture"}

            def build(self, workspace, version, provenance):
                (workspace / "downloaded").write_text("temporary")
                raise framework.RecipeError("fixture failed")

        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "Cargo.toml").write_text("[workspace]\n")
            (root / "upstream/sources.toml").parent.mkdir(parents=True)
            (root / "upstream/sources.toml").write_text("sources = []\n")
            with mock.patch.object(framework, "repo_root", return_value=root), \
                 mock.patch.object(framework, "repository_versions", return_value=[]):
                with self.assertRaises(framework.RecipeError):
                    framework.run_recipe(FailingRecipe(), ["update"], root / "recipe.py")
            self.assertEqual(list((root / "out/tmp").iterdir()), [])

    def test_publish_failure_also_cleans_workspace_and_keeps_no_deb(self):
        class FixtureRecipe(framework.PackageRecipe):
            name = "fixture"
            repository = "mattos"
            description = "Fixture"

            def discover_version(self):
                return "1.0", {"source": "fixture"}

            def build(self, workspace, version, provenance):
                staging = workspace / "package"
                (staging / "usr/bin").mkdir(parents=True)
                (staging / "usr/bin/fixture").write_text("fixture\n")
                return framework.finalize_package(self, staging, workspace, version, provenance)

        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "Cargo.toml").write_text("[workspace]\n")
            (root / "upstream/sources.toml").parent.mkdir(parents=True)
            (root / "upstream/sources.toml").write_text("sources = []\n")
            with mock.patch.object(framework, "repo_root", return_value=root), \
                 mock.patch.object(framework, "repository_versions", return_value=[]), \
                 mock.patch.object(framework, "publish", side_effect=framework.RecipeError("upload failed")):
                with self.assertRaises(framework.RecipeError):
                    framework.run_recipe(FixtureRecipe(), ["publish"], root / "recipe.py")
            self.assertEqual(list((root / "out/tmp").iterdir()), [])

    def test_recipes_are_outside_the_core_dag(self):
        for recipe in (ROOT / "third-party-packages/firefox.py", ROOT / "third-party-packages/fastfetch.py"):
            text = recipe.read_text(encoding="utf-8")
            self.assertNotIn("BuildStage", text)
            self.assertIn(
                "TemporaryDirectory",
                (ROOT / "third-party-packages/common/framework.py").read_text(encoding="utf-8"),
            )


if __name__ == "__main__":
    unittest.main()
