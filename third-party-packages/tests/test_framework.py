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
import common.build as build_module
import common.discovery as discovery
import common


class FrameworkTests(unittest.TestCase):
    @staticmethod
    def write_selection(root: Path, name: str = "fixture", version: str = "1.0") -> None:
        path = root / "third-party-packages/releases.json"
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(json.dumps({"format": 1, "packages": {name: {"version": version}}}))
    def test_common_exports_match_framework_symbols(self):
        self.assertEqual(len(common.__all__), len(set(common.__all__)))
        for name in common.__all__:
            self.assertTrue(hasattr(common, name), name)
            self.assertTrue(hasattr(framework, name), name)

    def test_git_latest_tag_picks_the_highest_matching_release(self):
        listing = "\n".join(f"{i}\trefs/tags/{tag}" for i, tag in enumerate(
            ["v1.9.0", "v1.10.0", "v1.10.0-rc1", "20201217", "release-2.32.10"]))
        with mock.patch.object(discovery, "command", return_value=listing):
            self.assertEqual(framework.git_latest_tag("u", r"v([0-9]+\.[0-9]+\.[0-9]+)"), ("1.10.0", "v1.10.0"))
            self.assertEqual(framework.git_latest_tag("u", r"release-(2\.[0-9.]+)"), ("2.32.10", "release-2.32.10"))
            with self.assertRaises(framework.RecipeError):
                framework.git_latest_tag("u", r"x([0-9]+)")

    def test_git_latest_tag_retries_a_transient_failure(self):
        listing = "0\trefs/tags/v1.2.0"
        with mock.patch.object(discovery, "command",
                               side_effect=[framework.RecipeError("reset"), framework.RecipeError("reset"), listing]), \
             mock.patch.object(discovery.time, "sleep"):
            self.assertEqual(framework.git_latest_tag("u", r"v([0-9.]+)"), ("1.2.0", "v1.2.0"))
        with mock.patch.object(discovery, "command", side_effect=framework.RecipeError("down")), \
             mock.patch.object(discovery.time, "sleep"), self.assertRaises(framework.RecipeError):
            framework.git_latest_tag("u", r"v([0-9.]+)")

    def test_build_dependencies_resolve_to_their_selected_published_package(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.write_selection(root, "libfixture", "2.0")

            class Recipe(framework.PackageRecipe):
                name = "user"
                repository = "mattos"
                build_depends = ("libfixture",)

            published = framework.PublishedPackage("2.0-0mattos1", "inputs", "pool/l/libfixture.deb")
            inventory = {"libfixture": [framework.PublishedPackage("1.0-0mattos1", "", "old.deb"), published]}
            self.assertEqual(framework.resolve_build_dependencies(root, Recipe(), inventory),
                             [("libfixture", published)])
            with self.assertRaises(framework.RecipeError):
                framework.resolve_build_dependencies(root, Recipe(), {"libfixture": inventory["libfixture"][:1]})
            # A dependency's published build changes the dependent's inputs.
            script = Path(framework.__file__)
            selection = framework.ReleaseSelection("1.0", {})
            first = framework.build_inputs_digest(Recipe(), script, selection, [("libfixture", published)])
            rebuilt = framework.PublishedPackage("2.0-0mattos1", "other", "pool/l/libfixture.deb")
            self.assertNotEqual(first, framework.build_inputs_digest(Recipe(), script, selection,
                                                                     [("libfixture", rebuilt)]))

    def test_packages_index_records_filenames(self):
        index = "Package: a\nVersion: 1\nFilename: pool/a_1_amd64.deb\n\nPackage: b\nVersion: 2\n"
        parsed = framework.parse_packages_index(index)
        self.assertEqual(parsed["a"][0].filename, "pool/a_1_amd64.deb")
        self.assertEqual(parsed["b"][0].filename, "")

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
        with mock.patch.object(discovery, "urlopen", return_value=response):
            version, provenance = framework.github_latest_release("fastfetch-cli", "fastfetch")
        self.assertEqual(version, "2.68.1")
        self.assertEqual(provenance["release_tag"], "v2.68.1")

        bad_response = Response(b"[]")
        with mock.patch.object(discovery, "urlopen", return_value=bad_response), \
             mock.patch.object(discovery.time, "sleep"):
            with self.assertRaises(framework.RecipeError):
                framework.github_latest_release("owner", "missing")

    RECIPES = ("btop.py", "fastfetch.py", "htop.py")

    def test_real_recipe_modules_have_required_contract_metadata(self):
        selections = framework.release_selections(ROOT)
        for filename in self.RECIPES:
            with self.subTest(filename=filename):
                namespace = self.load_recipe(filename)
                recipe_types = [value for value in namespace.values()
                                if isinstance(value, type) and issubclass(value, framework.PackageRecipe)
                                and value not in (framework.PackageRecipe, framework.SourceReleaseRecipe)]
                self.assertEqual(len(recipe_types), 1)
                recipe = recipe_types[0]()
                self.assertTrue(recipe.name)
                self.assertEqual(framework.validate_repository(recipe), "mattos")
                self.assertTrue(recipe.description)
                self.assertEqual(recipe.architecture, "amd64")
                self.assertTrue(recipe.dependency_names())
                # Every shipped recipe builds a verified, pinned source archive.
                self.assertIsInstance(recipe, framework.SourceReleaseRecipe)
                self.assertRegex(selections[recipe.name].provenance["source_sha256"], r"^[0-9a-f]{64}$")

    def test_real_recipe_no_argument_entrypoints_are_read_only_checks(self):
        for filename in self.RECIPES:
            with self.subTest(filename=filename):
                with mock.patch.object(framework, "repo_root", return_value=ROOT), \
                     mock.patch.object(framework, "published_packages", return_value=[]), \
                     mock.patch.object(framework, "github_latest_release", return_value=("9.9", {"release_tag": "v9.9"})), \
                     mock.patch.object(framework, "builder_image", side_effect=AssertionError("check built an image")), \
                     mock.patch.object(framework, "build_in_container", side_effect=AssertionError("check built")):
                    with mock.patch.object(sys, "argv", [str(PACKAGE_ROOT / filename)]):
                        with self.assertRaises(SystemExit) as exit_info:
                            runpy.run_path(str(PACKAGE_ROOT / filename), run_name="__main__")
                self.assertEqual(exit_info.exception.code, 0)

    def test_shipped_recipes_declare_mattos(self):
        for filename in self.RECIPES:
            recipe_type = next(value for value in self.load_recipe(filename).values()
                               if isinstance(value, type) and issubclass(value, framework.PackageRecipe)
                               and value not in (framework.PackageRecipe, framework.SourceReleaseRecipe))
            self.assertEqual(framework.validate_repository(recipe_type()), "mattos")

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

    def test_repository_index_comes_from_the_publishers_own_configuration(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            publisher = root / framework.PUBLISHER_RELATIVE
            publisher.parent.mkdir(parents=True)
            publisher.write_text(
                "from dataclasses import dataclass\n"
                "@dataclass(frozen=True)\n"
                "class Unused:\n"
                "    value: str = ''\n"
                "class Config:\n"
                "    def __init__(self, url, suite):\n"
                "        self.public_url, self.suite, self.component = url, suite, 'main'\n"
                "    @classmethod\n"
                "    def from_env(cls, repository):\n"
                "        return cls('https://' + repository + '.example', 'trixie' if repository == 'mattos' else 'stable')\n"
            )
            self.assertEqual(framework.repository_index_url(root, "mattos"),
                             "https://mattos.example/dists/trixie/main/binary-amd64/Packages.gz")
            self.assertEqual(framework.repository_index_url(root, "mattpackages"),
                             "https://mattpackages.example/dists/stable/main/binary-amd64/Packages.gz")

    def test_index_parsing_keeps_versions_and_build_inputs(self):
        index = framework.parse_packages_index(
            "Package: htop\nVersion: 3.5.3-0mattos1\nX-MattOS-Build-Inputs: abc\nDescription: x\n continued: no\n\n"
            "Package: btop\nVersion: 1.4.7\n"
        )
        self.assertEqual(index["htop"], [framework.PublishedPackage("3.5.3-0mattos1", "abc")])
        self.assertEqual(index["btop"], [framework.PublishedPackage("1.4.7", "")])

    def test_index_parsing_keeps_the_build_environment_and_depends(self):
        index = framework.parse_packages_index(
            "Package: wget\nVersion: 1.25-0mattos1\nDepends: libc6 (>= 2.42), libssl3t64, libidn2-0 | libidn2\n"
            "X-MattOS-Build-Environment: env\nDescription: x\n"
        )
        entry = index["wget"][0]
        self.assertEqual(entry.build_environment, "env")
        self.assertEqual(entry.depends, ("libc6 (>= 2.42)", "libssl3t64", "libidn2-0 | libidn2"))
        self.assertEqual(framework.dependency_package_names(entry.depends),
                         ["libc6", "libidn2", "libidn2-0", "libssl3t64"])
        with tempfile.TemporaryDirectory() as directory:
            snapshot = Path(directory) / "inventory.json"
            framework.write_repository_inventory(snapshot, "mattos", index)
            self.assertEqual(framework.read_repository_inventory(snapshot, "mattos"), index)

    def test_build_environment_counts_only_linked_packages_and_the_toolchain(self):
        class C(framework.PackageRecipe):
            name = "wget"
            repository = "mattos"

        class Rust(C):
            toolchains = ("rust",)

        class Go(C):
            toolchains = ("go",)

        image = {name: "1" for name in (*framework.TOOLCHAIN_PACKAGES["rust"], "libc6", "libssl3t64", "libidn2-0",
                                       "libsqlite3-0", "python3")}

        def changed(**shas):
            return {**image, **shas}

        depends = ("libc6 (>= 2.42)", "libssl3t64", "conmon")  # conmon is third-party: not in the image
        digest = framework.build_environment_digest
        for recipe in (C(), Rust(), Go()):
            base = digest(recipe, depends, image)
            with self.subTest(recipe=type(recipe).__name__):
                self.assertRegex(base, r"^[0-9a-f]{64}$")
                # A library the package does not link, or an unrelated tool.
                self.assertEqual(base, digest(recipe, depends, changed(libsqlite3_0="2", python3="2")))
                self.assertEqual(base, digest(recipe, depends, changed(**{"libsqlite3-0": "2"})))
                # A library it links.
                self.assertNotEqual(base, digest(recipe, depends, changed(libssl3t64="2")))
        # The compilers count only for the toolchains a recipe declares.
        self.assertNotEqual(digest(C(), depends, image), digest(C(), depends, changed(gcc="2")))
        self.assertEqual(digest(C(), depends, image), digest(C(), depends, changed(rustc="2")))
        self.assertNotEqual(digest(Rust(), depends, image), digest(Rust(), depends, changed(rustc="2")))
        self.assertEqual(digest(Go(), depends, image), digest(Go(), depends, changed(gcc="2", rustc="2")))
        self.assertEqual(digest(C(), depends, {}), "")
        bad = type("Bad", (C,), {"toolchains": ("fortran",)})()
        with self.assertRaises(framework.RecipeError):
            digest(bad, depends, image)

    def test_release_selection_revision_and_states_are_explicit(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            path = root / "third-party-packages/releases.json"
            path.parent.mkdir(parents=True)
            path.write_text(json.dumps({"format": 1, "packages": {
                "fixture": {"version": "1.0", "revision": 2, "source_sha256": "a" * 64}}}))
            selected = framework.selected_release(root, type("Fixture", (framework.PackageRecipe,), {"name": "fixture"})())
            self.assertEqual((selected.version, selected.package_version), ("1.0", "1.0-0mattos2"))
            for bad in ({"version": "1", "revision": 0}, {"version": "1", "source_sha256": "XYZ"}):
                path.write_text(json.dumps({"format": 1, "packages": {"fixture": bad}}))
                with self.subTest(bad=bad), self.assertRaises(framework.RecipeError):
                    framework.release_selections(root)
        state = framework.release_state
        self.assertEqual(state("1.1", "1.0", ["1.0-0mattos1"], selected_package_version="1.0-0mattos1"),
                         ("upstream-newer", "1.0-0mattos1"))
        self.assertEqual(state("1.0", "1.0", []), ("pending-publish", None))
        self.assertEqual(state("1.0", "1.0", ["1.0-0mattos1"], selected_package_version="1.0-0mattos1",
                               published_inputs="old", build_inputs="new"), ("rebuild-pending", "1.0-0mattos1"))
        self.assertEqual(state("1.0", "1.0", ["1.0-0mattos1"], selected_package_version="1.0-0mattos1",
                               published_inputs="same", build_inputs="same"), ("up-to-date", "1.0-0mattos1"))
        self.assertEqual(state("1.0", "1.0", ["1.0-0mattos1"], selected_package_version="1.0-0mattos1",
                               published_inputs="same", build_inputs="same",
                               published_environment="old", build_environment="new"),
                         ("stale-environment", "1.0-0mattos1"))
        # A stale environment is reported even when upstream moved on.
        self.assertEqual(state("1.1", "1.0", ["1.0-0mattos1"], selected_package_version="1.0-0mattos1",
                               published_inputs="same", build_inputs="same",
                               published_environment="old", build_environment="new")[0], "stale-environment")
        # Changed build inputs win over a stale environment; an unknown
        # environment (an older publication) is not stale.
        self.assertEqual(state("1.0", "1.0", ["1.0-0mattos1"], selected_package_version="1.0-0mattos1",
                               published_inputs="old", build_inputs="new",
                               published_environment="old", build_environment="new")[0], "rebuild-pending")
        self.assertEqual(state("1.0", "1.0", ["1.0-0mattos1"], selected_package_version="1.0-0mattos1",
                               published_inputs="same", build_inputs="same",
                               published_environment="", build_environment="new")[0], "up-to-date")

    def test_build_inputs_cover_the_recipe_selection_and_build_code_only(self):
        recipe = self.fixture_recipe()()
        with tempfile.TemporaryDirectory() as directory:
            script = Path(directory) / "fixture.py"
            script.write_text("# v1\n")
            build_code = Path(directory) / "build.py"
            build_code.write_text("# build v1\n")
            selection = framework.ReleaseSelection("1.0", {"source_sha256": "a" * 64})
            with mock.patch.object(framework.build_module, "__file__", str(build_code)):
                base = framework.build_inputs_digest(recipe, script, selection)
                self.assertEqual(base, framework.build_inputs_digest(recipe, script, selection))
                self.assertNotEqual(base, framework.build_inputs_digest(
                    recipe, script, framework.ReleaseSelection("1.0", {"source_sha256": "a" * 64}, 2)))
                # The image is not an input; neither is any host-only code.
                with mock.patch.object(framework, "image_package_sha256", return_value={"libc6": "new"}):
                    self.assertEqual(base, framework.build_inputs_digest(recipe, script, selection))
                build_code.write_text("# build v2\n")
                self.assertNotEqual(base, framework.build_inputs_digest(recipe, script, selection))
                build_code.write_text("# build v1\n")
                script.write_text("# v2\n")
                self.assertNotEqual(base, framework.build_inputs_digest(recipe, script, selection))
        common_dir = Path(framework.__file__).resolve().parent
        self.assertEqual(framework.build_code_files(recipe), [common_dir / "build.py"])
        go_recipe = type("GoFixture", (type(recipe),), {"toolchains": ("go",)})()
        self.assertEqual(framework.build_code_files(go_recipe), [common_dir / "build.py", common_dir / "go.py"])
        hashed = {path.name for path in framework.build_code_files(go_recipe)}
        self.assertTrue(hashed.isdisjoint({"framework.py", "discovery.py", "__init__.py"}))

    def test_build_code_does_not_import_host_only_modules(self):
        # Whatever build.py imports at module level runs in every build and
        # would escape the fingerprint.
        import ast
        tree = ast.parse(Path(build_module.__file__).read_text(encoding="utf-8"))
        imported = {node.module for node in tree.body if isinstance(node, ast.ImportFrom) and node.level}
        self.assertEqual(imported, set())

    def test_repository_inventory_snapshot_avoids_repeated_remote_lookup(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            snapshot = root / "inventory.json"
            framework.write_repository_inventory(
                snapshot, "mattos", {"fixture": [framework.PublishedPackage("1.0-0mattos1", "abc")]})
            with mock.patch.object(framework, "repository_inventory", side_effect=AssertionError("remote lookup")):
                self.assertEqual(framework.published_packages(root, "fixture", "mattos", snapshot),
                                 [framework.PublishedPackage("1.0-0mattos1", "abc")])
            snapshot.write_text('{"format": 3, "repository": "mattpackages", "packages": {}}')
            with self.assertRaises(framework.RecipeError):
                framework.repository_versions(root, "fixture", "mattos", snapshot)

    def test_repository_lookup_failure_is_not_version_absent(self):
        with mock.patch.object(framework, "repository_index_url", return_value="https://example.invalid/Packages.gz"), \
             mock.patch.object(framework, "urlopen", side_effect=OSError("offline")):
            with self.assertRaises(framework.RecipeError):
                framework.repository_versions(Path("/repo"), "fastfetch", "mattos")

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

    def test_container_sees_only_its_workspace_and_the_read_only_recipes(self):
        class FixtureRecipe(framework.PackageRecipe):
            name = "fixture"
            repository = "mattos"

        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            recipes = root / "third-party-packages"
            recipes.mkdir()
            script = recipes / "fixture.py"
            script.write_text("# fixture\n")
            workspace = root / "out/tmp/workspace"
            workspace.mkdir(parents=True)

            def fake_command(args, **_kwargs):
                if args[1] == "run":
                    (workspace / "fixture_1.0_amd64.deb").write_bytes(b"deb")
                    (workspace / "container-result.json").write_text(json.dumps({"artifact": "fixture_1.0_amd64.deb"}))
                return ""

            with mock.patch.object(framework, "container_engine", return_value="podman"), \
                 mock.patch.object(framework, "command", side_effect=fake_command) as invoke:
                result = framework.build_in_container(root, FixtureRecipe(), script, workspace, "1.0",
                                                      {"source": "fixture"}, ("localhost/mattos-builder:x", "sha256:x"))
            self.assertEqual(result.artifact, workspace / "fixture_1.0_amd64.deb")
            run_args = next(call.args[0] for call in invoke.call_args_list if call.args[0][1] == "run")
            self.assertNotIn("upload", run_args)
            self.assertIn("__container-build", run_args)
            self.assertIn("localhost/mattos-builder:x", run_args)
            volumes = [run_args[index + 1] for index, value in enumerate(run_args) if value == "--volume"]
            self.assertEqual(volumes, [f"{workspace.resolve()}:/work:rw", f"{recipes.resolve()}:/recipes:ro"])
            self.assertIn("/recipes/fixture.py", run_args)

    def test_rootless_podman_under_no_new_privs_gets_a_clear_error(self):
        with mock.patch.object(framework, "no_new_privileges", return_value=True), \
             mock.patch.object(framework.os, "getuid", return_value=1000):
            with self.assertRaisesRegex(framework.RecipeError, "no_new_privs"):
                framework.ensure_rootless_engine_can_start("podman")
            framework.ensure_rootless_engine_can_start("docker")
        with mock.patch.object(framework, "no_new_privileges", return_value=False):
            framework.ensure_rootless_engine_can_start("podman")

    def test_builder_image_is_loaded_only_when_the_engine_lacks_it(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            metadata = root / framework.BUILDER_IMAGE_METADATA
            metadata.parent.mkdir(parents=True)
            metadata.write_text(json.dumps({"reference": "localhost/mattos-builder:abc",
                                            "manifest_digest": "sha256:abc",
                                            "archive": "out/images/mattos-builder.oci.tar"}))
            for present, loads in ((0, 0), (1, 1)):
                with self.subTest(present=present), \
                     mock.patch.object(framework, "command") as run, \
                     mock.patch.object(framework, "ensure_rootless_engine_can_start"), \
                     mock.patch.object(framework.subprocess, "run", return_value=mock.Mock(returncode=present)):
                    self.assertEqual(framework.builder_image(root, "podman"),
                                     ("localhost/mattos-builder:abc", "sha256:abc"))
                commands = [call.args[0] for call in run.call_args_list]
                self.assertEqual(commands[0][-1], "builder-image")
                self.assertEqual(sum(1 for args in commands if args[1] == "load"), loads)
            self.assertEqual(framework.image_package_sha256(root), {})
            metadata.write_text(json.dumps({"package_sha256": {"libc6": "abc"}}))
            self.assertEqual(framework.image_package_sha256(root), {"libc6": "abc"})

    def test_elf_dependencies_map_loaded_libraries_to_their_packages(self):
        with tempfile.TemporaryDirectory() as directory:
            staging = Path(directory)
            (staging / "usr/bin").mkdir(parents=True)
            (staging / "usr/bin/tool").write_bytes(b"\x7fELF fixture")
            (staging / "usr/bin/script").write_text("#!/bin/sh\n")
            owners = {"/usr/lib/x86_64-linux-gnu/libncursesw.so.6": "libncursesw6",
                      "/usr/lib/x86_64-linux-gnu/libc.so.6": "libc6"}

            def fake_run(args, **_kwargs):
                if args[0] == "ldd":
                    return mock.Mock(stdout="\tlibncursesw.so.6 => /usr/lib/x86_64-linux-gnu/libncursesw.so.6 (0x1)\n"
                                            "\tlibc.so.6 => /usr/lib/x86_64-linux-gnu/libc.so.6 (0x2)\n")
                owner = owners.get(args[2])
                return mock.Mock(returncode=0 if owner else 1, stdout=f"{owner}: {args[2]}\n" if owner else "")

            with mock.patch.object(framework.subprocess, "run", side_effect=fake_run), \
                 mock.patch.object(framework.os.path, "realpath", side_effect=lambda path: path):
                self.assertEqual(framework.elf_runtime_dependencies(staging), ["libc6", "libncursesw6"])

            def missing(args, **_kwargs):
                return mock.Mock(stdout="\tlibidn2.so.0 => not found\n", returncode=0)

            with mock.patch.object(framework.subprocess, "run", side_effect=missing), \
                 self.assertRaises(framework.RecipeError):
                framework.elf_runtime_dependencies(staging)

    def test_mattos_package_names_are_refused(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            inventory = root / framework.MATTOS_INVENTORY
            inventory.parent.mkdir(parents=True)
            inventory.write_text('[[package]]\nname = "rsync"\n\n[[package]]\nname = "libc6"\n')
            mattos = type("Rsync", (framework.PackageRecipe,), {"name": "rsync", "repository": "mattos"})()
            other = type("Btop", (framework.PackageRecipe,), {"name": "btop", "repository": "mattos"})()
            with self.assertRaises(framework.RecipeError):
                framework.ensure_not_a_mattos_package(root, mattos)
            framework.ensure_not_a_mattos_package(root, other)
            (inventory).unlink()
            with self.assertRaises(framework.RecipeError):
                framework.ensure_not_a_mattos_package(root, other)

    def test_source_release_recipe_verifies_its_pinned_archive(self):
        framework.require_tools(["dpkg-deb", "make", "cc"])
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            tree = root / "fixture-1.0"
            tree.mkdir()
            (tree / "Makefile").write_text(
                "all:\n\tprintf '#!/bin/sh\\necho fixture\\n' > fixture\n"
                "install:\n\tinstall -D -m 0755 fixture $(DESTDIR)$(PREFIX)/bin/fixture\n"
            )
            archive = root / "fixture-1.0.tar.gz"
            import tarfile
            with tarfile.open(archive, "w:gz") as tar:
                tar.add(tree, arcname="fixture-1.0")

            class Fixture(framework.SourceReleaseRecipe):
                name = "fixture"
                repository = "mattos"
                description = "Fixture"
                depends = ("libc6",)
                source_url = archive.as_uri().replace("fixture-1.0", "fixture-{version}")
                build_system = "make"
                install_options = ("PREFIX=/usr",)

            good = {"upstream_version": "1.0", "source_sha256": framework.sha256_file(archive)}
            workspace = root / "work"
            workspace.mkdir()
            result = Fixture().build(workspace, "1.0-0mattos1", good)
            contents = framework.command(["dpkg-deb", "--contents", str(result.artifact)])
            self.assertIn("./usr/bin/fixture", contents)
            bad = root / "bad"
            bad.mkdir()
            with self.assertRaises(framework.RecipeError):
                Fixture().build(bad, "1.0-0mattos1", {**good, "source_sha256": "0" * 64})
            with self.assertRaises(framework.RecipeError):
                Fixture().build(bad, "1.0-0mattos1", {"upstream_version": "1.0"})

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

    IMAGE_PACKAGES = {"libc6": "1", "gcc": "1", "libsqlite3-0": "1"}

    def lifecycle(self, root: Path, published=(), image=("localhost/mattos-builder:x", "sha256:x"),
                  package_sha256=None):
        """Patches around run_recipe: an in-process build, a fixed image and
        a MattOS inventory that does not contain the fixture."""
        (root / "Cargo.toml").write_text("[workspace]\n")
        (root / "upstream/sources.toml").parent.mkdir(parents=True, exist_ok=True)
        (root / "upstream/sources.toml").write_text("sources = []\n")
        (root / "recipe.py").write_text("# fixture recipe\n")
        inventory = root / framework.MATTOS_INVENTORY
        inventory.parent.mkdir(parents=True, exist_ok=True)
        inventory.write_text('[[package]]\nname = "libc6"\n')
        metadata = root / framework.BUILDER_IMAGE_METADATA
        metadata.parent.mkdir(parents=True, exist_ok=True)
        metadata.write_text(json.dumps({"reference": image[0], "manifest_digest": image[1], "archive": "x",
                                        "package_sha256": package_sha256 or self.IMAGE_PACKAGES}))

        def build_in_container(_root, recipe, _script, workspace, version, provenance, _image=None,
                               package_sha256=None):
            # What container_build does, in process.
            build_module._image_package_sha256.clear()
            build_module._image_package_sha256.update(package_sha256 or {})
            return recipe.build(workspace, version, provenance)

        return [
            mock.patch.object(framework, "repo_root", return_value=root),
            mock.patch.object(framework, "published_packages", return_value=list(published)),
            mock.patch.object(framework, "container_engine", return_value="podman"),
            mock.patch.object(framework, "builder_image", return_value=image),
            mock.patch.object(framework, "build_in_container", side_effect=build_in_container),
        ]

    @staticmethod
    def fixture_recipe(fail=None):
        class FixtureRecipe(framework.PackageRecipe):
            name = "fixture"
            repository = "mattos"
            description = "Fixture"
            depends = ("libc6",)
            built = []

            def discover_version(self):
                return "1.0", {"source": "fixture"}

            def build(self, workspace, version, provenance):
                FixtureRecipe.built.append(version)
                if fail:
                    (workspace / "downloaded").write_text("temporary")
                    raise framework.RecipeError(fail)
                staging = workspace / "package"
                (staging / "usr/bin").mkdir(parents=True)
                (staging / "usr/bin/fixture").write_text("fixture\n")
                return framework.finalize_package(self, staging, workspace, version, provenance)

        return FixtureRecipe

    def run_with(self, patches, recipe, argv, root):
        import contextlib
        with contextlib.ExitStack() as stack:
            for patch in patches:
                stack.enter_context(patch)
            return framework.run_recipe(recipe, argv, root / "recipe.py")

    def test_local_build_does_not_query_repository_and_persists_only_deb(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.write_selection(root)
            patches = self.lifecycle(root)
            patches[1] = mock.patch.object(framework, "published_packages", side_effect=AssertionError("build queried repository"))
            destination = root / "dist"
            self.assertEqual(self.run_with(patches, self.fixture_recipe()(), ["build", "--output", str(destination)], root), 0)
            self.assertTrue((destination / "fixture_1.0-0mattos1_amd64.deb").is_file())
            fields = framework.command(["dpkg-deb", "--field", str(destination / "fixture_1.0-0mattos1_amd64.deb"),
                                        "X-MattOS-Build-Inputs"]).strip()
            self.assertRegex(fields, r"^[0-9a-f]{64}$")
            environment = framework.command(["dpkg-deb", "--field", str(destination / "fixture_1.0-0mattos1_amd64.deb"),
                                             "X-MattOS-Build-Environment"]).strip()
            self.assertEqual(environment, framework.build_environment_digest(
                self.fixture_recipe()(), ("libc6",), self.IMAGE_PACKAGES))
            self.assertEqual(list((root / "out/tmp").iterdir()), [])

    def test_no_argument_invocation_is_read_only_check(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.write_selection(root)
            patches = self.lifecycle(root)
            patches[3] = mock.patch.object(framework, "builder_image", side_effect=AssertionError("check built an image"))
            recipe = self.fixture_recipe()
            self.assertEqual(self.run_with(patches, recipe(), [], root), 0)
            self.assertEqual(recipe.built, [])

    def test_update_skips_only_identical_published_builds(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.write_selection(root)
            self.lifecycle(root)  # writes the fixture recipe file the digest covers
            selection = framework.ReleaseSelection("1.0", {}, 1)
            identical = framework.build_inputs_digest(self.fixture_recipe()(), root / "recipe.py", selection)
            for published_inputs, builds in ((identical, 0), ("stale", 1), (None, 1)):
                published = [] if published_inputs is None else [framework.PublishedPackage("1.0-0mattos1", published_inputs)]
                patches = self.lifecycle(root, published)
                recipe = self.fixture_recipe()
                with self.subTest(published=published_inputs), mock.patch.object(framework, "publish") as upload:
                    self.assertEqual(self.run_with(patches, recipe(), ["update"], root), 0)
                self.assertEqual(len(recipe.built), builds)
                self.assertEqual(upload.call_count, builds)

    def test_an_older_builder_image_makes_a_package_stale_not_rebuilt(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.write_selection(root)
            self.lifecycle(root)
            recipe_class = self.fixture_recipe()
            selection = framework.ReleaseSelection("1.0", {}, 1)
            inputs = framework.build_inputs_digest(recipe_class(), root / "recipe.py", selection)
            built_with = framework.build_environment_digest(recipe_class(), ("libc6",), self.IMAGE_PACKAGES)
            published = [framework.PublishedPackage("1.0-0mattos1", inputs, "pool/f.deb", built_with, ("libc6",))]
            cases = (
                # (image packages now, arguments, rebuilt, status)
                (self.IMAGE_PACKAGES, ["update"], 0, "up-to-date"),
                ({**self.IMAGE_PACKAGES, "libsqlite3-0": "2"}, ["update"], 0, "up-to-date"),  # not linked
                ({**self.IMAGE_PACKAGES, "libc6": "2"}, ["update"], 0, "stale-environment"),
                ({**self.IMAGE_PACKAGES, "gcc": "2"}, ["update"], 0, "stale-environment"),  # its compiler
                ({**self.IMAGE_PACKAGES, "libc6": "2"}, ["update", "--rebuild-stale"], 1, "uploaded"),
                ({**self.IMAGE_PACKAGES, "libc6": "2"}, ["check"], 0, "checked"),
            )
            for image, argv, rebuilt, status in cases:
                result = root / "result.json"
                patches = self.lifecycle(root, published, package_sha256=image)
                recipe = self.fixture_recipe()
                with self.subTest(image=image, argv=argv), mock.patch.object(framework, "publish") as upload:
                    self.assertEqual(self.run_with(patches, recipe(), [*argv, "--result-json", str(result)], root), 0)
                    data = json.loads(result.read_text())
                    self.assertEqual(data["status"], status)
                    if status == "checked":
                        self.assertEqual(data["release_state"], "stale-environment")
                    self.assertEqual(len(recipe.built), rebuilt)
                    self.assertEqual(upload.call_count, rebuilt)

    def test_publishing_a_mattos_package_name_is_refused(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.write_selection(root)
            patches = self.lifecycle(root)
            (root / framework.MATTOS_INVENTORY).write_text('[[package]]\nname = "fixture"\n')
            recipe = self.fixture_recipe()
            with mock.patch.object(framework, "publish") as upload, self.assertRaises(framework.RecipeError):
                self.run_with(patches, recipe(), ["update"], root)
            upload.assert_not_called()
            self.assertEqual(recipe.built, [])

    def test_failed_update_cleans_ephemeral_workspace(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.write_selection(root)
            with self.assertRaises(framework.RecipeError):
                self.run_with(self.lifecycle(root), self.fixture_recipe("fixture failed")(), ["update"], root)
            self.assertEqual(list((root / "out/tmp").iterdir()), [])

    def test_publish_failure_cleans_workspace_and_keeps_the_package_for_the_next_update(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.write_selection(root)
            kept = root / framework.UNPUBLISHED_ARTIFACTS / "fixture_1.0-0mattos1_amd64.deb"
            recipe = self.fixture_recipe()
            patches = self.lifecycle(root) + [mock.patch.object(
                framework, "publish", side_effect=framework.RecipeError("command failed\nError [remote]: unreachable"))]
            with self.assertRaises(framework.RecipeError) as raised:
                self.run_with(patches, recipe(), ["update"], root)
            self.assertEqual(list((root / "out/tmp").iterdir()), [])
            self.assertTrue(kept.is_file())
            last = str(raised.exception).splitlines()[-1]
            self.assertTrue(last.startswith("error: upload failed (Error [remote]: unreachable)"), last)
            self.assertIn(str(kept.relative_to(root)), last)
            self.assertEqual(len(recipe.built), 1)
            # The repository is back: the kept package is published, not rebuilt.
            with mock.patch.object(framework, "publish") as upload:
                self.assertEqual(self.run_with(self.lifecycle(root), recipe(), ["update"], root), 0)
            self.assertEqual(len(recipe.built), 1)
            self.assertEqual(upload.call_args.args[1], kept)
            self.assertFalse(kept.is_file())

    def test_a_kept_package_is_rebuilt_when_it_no_longer_matches(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.write_selection(root)
            kept = root / framework.UNPUBLISHED_ARTIFACTS / "fixture_1.0-0mattos1_amd64.deb"
            recipe = self.fixture_recipe()
            with self.assertRaises(framework.RecipeError):
                self.run_with(self.lifecycle(root) + [mock.patch.object(
                    framework, "publish", side_effect=framework.RecipeError("down"))], recipe(), ["update"], root)
            self.assertTrue(kept.is_file())
            # A linked library changed in the image since: rebuild, then the
            # kept package is replaced by the published one.
            changed = {**self.IMAGE_PACKAGES, "libc6": "2"}
            with mock.patch.object(framework, "publish") as upload:
                self.assertEqual(self.run_with(self.lifecycle(root, package_sha256=changed), recipe(), ["update"],
                                               root), 0)
            self.assertEqual(len(recipe.built), 2)
            self.assertNotEqual(upload.call_args.args[1], kept)
            self.assertFalse(kept.is_file())
            # A dry run never keeps a package.
            with self.assertRaises(framework.RecipeError):
                self.run_with(self.lifecycle(root) + [mock.patch.object(
                    framework, "publish", side_effect=framework.RecipeError("down"))], recipe(),
                    ["update", "--dry-run"], root)
            self.assertFalse(kept.is_file())

    def test_recipes_are_outside_the_core_dag(self):
        for recipe in self.RECIPES:
            text = (PACKAGE_ROOT / recipe).read_text(encoding="utf-8")
            self.assertNotIn("BuildStage", text)
        self.assertIn("TemporaryDirectory", (PACKAGE_ROOT / "common/framework.py").read_text(encoding="utf-8"))

if __name__ == "__main__":
    unittest.main()
