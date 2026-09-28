import json
import sys
import unittest
from pathlib import Path
from tempfile import TemporaryDirectory
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import check_stage_reproducibility as reproducibility


def entry(path: str, content: str, size: int = 1, kind: str = "file", mode: int = 0o644) -> dict:
    return {"path": path, "kind": kind, "mode": mode, "uid": 0, "gid": 0, "size": size, "content": content}


def manifest(outputs: list[dict], output_digest: str, input_digest: str = "inputs") -> dict:
    return {"inputs": {"full_digest": input_digest}, "output_content_digest": output_digest, "expected_outputs": outputs}


class CompareTests(unittest.TestCase):
    def test_identical_inventories_have_no_differences(self) -> None:
        outputs = reproducibility.inventory(manifest([entry("a", "1")], "d"))
        self.assertEqual(reproducibility.compare_inventories(outputs, outputs), [])

    def test_every_kind_of_difference_is_reported(self) -> None:
        first = reproducibility.inventory(manifest([entry("gone", "1"), entry("dir", "x", size=2), entry("mode", "m")], "d1"))
        second = reproducibility.inventory(
            manifest([entry("new", "1"), entry("dir", "y", size=3), entry("mode", "m", mode=0o755)], "d2")
        )
        lines = "\n".join(reproducibility.compare_inventories(first, second))
        self.assertIn("only in the first build: gone", lines)
        self.assertIn("only in the second build: new", lines)
        self.assertIn("changed: dir (size, content; size 2 -> 3)", lines)
        self.assertIn("changed: mode (mode; mode 420 -> 493)", lines)

    def test_text_diffs_are_bounded_and_skip_binary(self) -> None:
        diff = reproducibility.text_diff(b"* GCC\n* gccinstall\n", b"* GCC\n", "info/dir")
        self.assertIn("-* gccinstall", diff)
        self.assertEqual(reproducibility.text_diff(b"\xff\xfe", b"\x00", "bin"), [])
        long = reproducibility.text_diff("\n".join(map(str, range(200))).encode(), b"", "long")
        self.assertEqual(long[-1], "  ...")
        self.assertEqual(reproducibility.text_diff(b"x" * (65 << 10), b"y", "big"), [])

    def test_snapshots_keep_only_small_files_within_budget(self) -> None:
        with TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "small").write_bytes(b"s")
            (root / "large").write_bytes(b"l" * (65 << 10))
            entries = {"small": entry("small", "c", size=1), "large": entry("large", "c", size=65 << 10), "link": entry("link", "t", kind="symlink")}
            self.assertEqual(reproducibility.snapshot_text_files(root, entries), {"small": b"s"})


class StageCheckTests(unittest.TestCase):
    def fake_build(self, root: Path, digests: list[str]):
        """Each ``build`` publishes the next digest; ``invalidate`` is a no-op."""
        calls = []

        def run(arguments, label):
            calls.append(arguments)
            if arguments[0] == "build":
                digest = digests[min(len([c for c in calls if c[0] == "build"]) - 1, len(digests) - 1)]
                path = reproducibility.manifest_path(root, arguments[1])
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text(json.dumps(manifest([entry("out/a", digest)], digest)), encoding="utf-8")

        return calls, run

    def test_reproducible_stage_passes_and_is_skipped_next_time(self) -> None:
        with TemporaryDirectory() as temporary:
            root = Path(temporary)
            calls, run = self.fake_build(root, ["same", "same", "same"])
            with mock.patch.object(reproducibility, "run", run):
                self.assertTrue(reproducibility.check_stage(root, "gcc-runtime", if_changed=True))
                self.assertIn(["cache", "invalidate", "gcc-runtime"], calls)
                calls.clear()
                self.assertTrue(reproducibility.check_stage(root, "gcc-runtime", if_changed=True))
                self.assertNotIn(["cache", "invalidate", "gcc-runtime"], calls, "verified inputs are not rebuilt")

    def test_nondeterministic_stage_fails_and_is_not_recorded(self) -> None:
        with TemporaryDirectory() as temporary:
            root = Path(temporary)
            _, run = self.fake_build(root, ["first", "second"])
            with mock.patch.object(reproducibility, "run", run), mock.patch("builtins.print"):
                self.assertFalse(reproducibility.check_stage(root, "glibc", if_changed=False))
            self.assertFalse(reproducibility.state_path(root, "glibc").exists())


if __name__ == "__main__":
    unittest.main()
