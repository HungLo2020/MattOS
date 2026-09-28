import sys
import unittest
from pathlib import Path
from tempfile import TemporaryDirectory

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import check_wiki_structure

REPO_ROOT = Path(__file__).resolve().parents[2]


def write(path: Path, text: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text, encoding="utf-8")


class WikiStructureTests(unittest.TestCase):
    def wiki(self, root: Path) -> None:
        write(root / "README.md", "[home](docs/index.md) [build](docs/build-system/index.md)\n")
        write(root / "docs/index.md", "# Home\n[Build](build-system/index.md)\n")
        write(root / "docs/assets/javascripts/random-page.js", "")
        write(root / "docs/build-system/index.md", "# Build\n[Building](building.md)\n[Toolchain](toolchain/index.md#top)\n")
        write(root / "docs/build-system/building.md", "# Building\n")
        write(root / "docs/build-system/toolchain/index.md", "# Toolchain\n[GCC](gcc.md)\n")
        write(root / "docs/build-system/toolchain/gcc.md", "# GCC\n")

    def test_a_complete_hierarchy_passes(self) -> None:
        with TemporaryDirectory() as temporary:
            root = Path(temporary)
            self.wiki(root)
            self.assertEqual(check_wiki_structure.check(root), [])

    def test_each_rule_reports_its_violation(self) -> None:
        with TemporaryDirectory() as temporary:
            root = Path(temporary)
            self.wiki(root)
            write(root / "docs/build-system/unlisted.md", "# Unlisted\n")
            write(root / "docs/build-system/Bad_Name.md", "# Bad\n")
            write(root / "docs/build-system/cache/notes.md", "# Orphan directory\n")
            write(root / "docs/packaging/index.md", "# Packaging\n")
            problems = "\n".join(check_wiki_structure.check(root))
            self.assertIn("docs/build-system/index.md: does not link to unlisted.md", problems)
            self.assertIn("Bad_Name.md: note name is not lowercase kebab-case", problems)
            self.assertIn("docs/build-system/cache: missing index.md", problems)
            self.assertIn("docs/build-system/index.md: does not link to cache/index.md", problems)
            self.assertIn("docs/index.md: does not link to packaging/index.md", problems)
            self.assertIn("README.md: does not link to docs/packaging/index.md", problems)

    def test_the_repository_wiki_follows_the_hierarchy(self) -> None:
        self.assertEqual(check_wiki_structure.check(REPO_ROOT), [])


if __name__ == "__main__":
    unittest.main()
