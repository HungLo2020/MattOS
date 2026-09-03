from __future__ import annotations

import sys
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT))
import common.framework as framework


class FrameworkTests(unittest.TestCase):
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
            framework.write_provenance(staging, {"source": "fixture", "version": "1.2.3"})
            artifact = framework.package_staging(staging, root / "dist", name="example", version="1.2.3")
            self.assertTrue(artifact.is_file())
            metadata = framework.command(["dpkg-deb", "--show", "--showformat=${Package} ${Version} ${Architecture}\n", str(artifact)])
            self.assertEqual(metadata.strip(), "example 1.2.3 amd64")

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
