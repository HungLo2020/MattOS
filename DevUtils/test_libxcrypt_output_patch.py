#!/usr/bin/env python3

from __future__ import annotations

import hashlib
from pathlib import Path
import shutil
import subprocess
import tempfile
import tomllib
import unittest


ROOT = Path(__file__).resolve().parents[1]
SOURCE = ROOT / "src/system/libraries/libxcrypt/lib/crypt-gost-yescrypt.c"
PATCH = ROOT / "upstream/patches/libxcrypt/0001-fix-discarded-qualifiers.patch"
MANIFEST = ROOT / "upstream/patches/libxcrypt/manifest.toml"
STATE = ROOT / "upstream/state/libxcrypt.toml"
PINNED_COMMIT = "55ea777e8d567e5e86ffac917c28815ac54cc341"
PINNED_TREE = "57d0673dc1358ce9ede887489c009cebf76a2043"
PRISTINE_SHA256 = "a4bca98dccf0b74a1a3d027faaef7c79209681b6b8908cf6dfd44ff721255dad"
PATCHED_SHA256 = "1cf5465bd2393615d2c7fd9f54bc3203b5ddf2d6831f32d1b8cd3651b0d5f62f"


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


class LibxcryptOutputPatchTests(unittest.TestCase):
    def test_pinned_source_remains_pristine_and_output_mirror_is_patched(self) -> None:
        with STATE.open("rb") as stream:
            state = tomllib.load(stream)
        with MANIFEST.open("rb") as stream:
            manifest = tomllib.load(stream)

        self.assertEqual(state["imported_commit"], PINNED_COMMIT)
        self.assertEqual(state["upstream_tree"], PINNED_TREE)
        self.assertEqual(state["patch_manifest_sha256"], sha256(MANIFEST))
        self.assertEqual(manifest["upstream_commit"], PINNED_COMMIT)
        self.assertEqual(manifest["upstream_tree"], PINNED_TREE)
        self.assertEqual(manifest["application"], "output-mirror-only")
        self.assertEqual(manifest["patch"][0]["sha256"], sha256(PATCH))

        pristine = SOURCE.read_bytes()
        self.assertEqual(hashlib.sha256(pristine).hexdigest(), PRISTINE_SHA256)
        self.assertIn(b"strchr ((const char *) intbuf->retval", pristine)

        output_root = ROOT / "out/tmp"
        output_root.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(prefix="libxcrypt-patch-test-", dir=output_root) as raw:
            mirror = Path(raw)
            mirrored_source = mirror / "lib/crypt-gost-yescrypt.c"
            mirrored_source.parent.mkdir(parents=True)
            shutil.copy2(SOURCE, mirrored_source)
            directory = mirror.relative_to(ROOT)
            completed = subprocess.run(
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
            self.assertEqual(completed.returncode, 0, completed.stdout)
            self.assertEqual(sha256(mirrored_source), PATCHED_SHA256)
            self.assertIn(b"strchr ((char *) intbuf->retval", mirrored_source.read_bytes())

        self.assertEqual(SOURCE.read_bytes(), pristine)


if __name__ == "__main__":
    unittest.main()