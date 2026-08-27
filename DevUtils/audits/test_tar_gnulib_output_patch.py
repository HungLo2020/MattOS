#!/usr/bin/env python3

from __future__ import annotations

import hashlib
from pathlib import Path
import shutil
import subprocess
import tempfile
import tomllib
import unittest


ROOT = Path(__file__).resolve().parents[2]
SOURCE = ROOT / "src/build-support/gnulib/lib/flexmember.h"
PATCH = ROOT / "upstream/patches/tar/0001-add-flexnsizeof-compatibility.patch"
MANIFEST = ROOT / "upstream/patches/tar/manifest.toml"
STATE = ROOT / "upstream/state/tar.toml"
GITLINK_POLICY = ROOT / "upstream/policies/gitlinks.toml"
PINNED_COMMIT = "e545d446dfe6564265cdf4186641ee76f4acc7fa"
PINNED_TREE = "ebf72a162689af8ff91729edb1c72c13f06ff0bb"
GNULIB_REPLACEMENT_COMMIT = "20932856a6a07f056918d58acd09cea4ba150a52"
PRISTINE_SHA256 = "1d1f2eb3cdeaf6f7ba37d32e22bc353a2ab1490c8f6e5e598e2ba1b3c9df7718"
PATCHED_SHA256 = "fa367968179f8644a9964b29ede3ea92764018bbcd8e34711c29a4cf26fef547"


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def compile_fixture(include_dir: Path, source: Path, output: Path) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [
            "cc",
            "-std=c11",
            "-Werror=implicit-function-declaration",
            f"-I{include_dir}",
            str(source),
            "-o",
            str(output),
        ],
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        check=False,
    )


class TarGnulibOutputPatchTests(unittest.TestCase):
    def test_flexnsizeof_failure_is_fixed_only_in_output_mirror(self) -> None:
        with STATE.open("rb") as stream:
            state = tomllib.load(stream)
        with MANIFEST.open("rb") as stream:
            manifest = tomllib.load(stream)
        with GITLINK_POLICY.open("rb") as stream:
            policies = tomllib.load(stream)

        self.assertEqual(state["imported_commit"], PINNED_COMMIT)
        self.assertEqual(state["upstream_tree"], PINNED_TREE)
        self.assertEqual(state["patch_manifest_sha256"], sha256(MANIFEST))
        self.assertEqual(manifest["upstream_commit"], PINNED_COMMIT)
        self.assertEqual(manifest["upstream_tree"], PINNED_TREE)
        self.assertEqual(manifest["application"], "output-mirror-only")
        self.assertEqual(manifest["patch"][0]["sha256"], sha256(PATCH))

        tar_policy = next(item for item in policies["component"] if item["name"] == "tar")
        gnulib_policy = next(item for item in tar_policy["gitlink"] if item["path"] == "gnulib")
        self.assertEqual(gnulib_policy["action"], "replacement")
        self.assertEqual(gnulib_policy["replacement_path"], "src/build-support/gnulib")
        self.assertEqual(gnulib_policy["replacement_commit"], GNULIB_REPLACEMENT_COMMIT)
        self.assertFalse(gnulib_policy["exact_gitlink_match"])
        self.assertEqual(manifest["patch"][0]["path"], PATCH.relative_to(ROOT).as_posix())
        self.assertIn(b"--- a/gnulib/lib/flexmember.h", PATCH.read_bytes())

        pristine = SOURCE.read_bytes()
        self.assertEqual(hashlib.sha256(pristine).hexdigest(), PRISTINE_SHA256)
        self.assertNotIn(b"FLEXNSIZEOF", pristine)

        output_root = ROOT / "out/tmp"
        output_root.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(prefix="tar-gnulib-patch-test-", dir=output_root) as raw:
            mirror = Path(raw)
            mirrored_header = mirror / "gnulib/lib/flexmember.h"
            mirrored_header.parent.mkdir(parents=True)
            shutil.copy2(SOURCE, mirrored_header)
            fixture = mirror / "flexnsizeof.c"
            fixture.write_text(
                '#include "flexmember.h"\n'
                "#include <stdlib.h>\n"
                "struct link { int value; char name[]; };\n"
                "int main(void) {\n"
                "  struct link *item = malloc(FLEXNSIZEOF(struct link, name, 4));\n"
                "  free(item);\n"
                "  return 0;\n"
                "}\n",
                encoding="ascii",
            )

            failed = compile_fixture(mirrored_header.parent, fixture, mirror / "before")
            self.assertNotEqual(failed.returncode, 0)
            self.assertIn("FLEXNSIZEOF", failed.stdout)
            self.assertIn("implicit declaration", failed.stdout)

            directory = mirror.relative_to(ROOT)
            applied = subprocess.run(
                [
                    "git",
                    "apply",
                    "--whitespace=error-all",
                    f"--directory={directory}",
                    str(PATCH),
                ],
                cwd=ROOT,
                text=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.STDOUT,
                check=False,
            )
            self.assertEqual(applied.returncode, 0, applied.stdout)
            self.assertEqual(sha256(mirrored_header), PATCHED_SHA256)

            compiled = compile_fixture(mirrored_header.parent, fixture, mirror / "after")
            self.assertEqual(compiled.returncode, 0, compiled.stdout)

        self.assertEqual(SOURCE.read_bytes(), pristine)


if __name__ == "__main__":
    unittest.main()
