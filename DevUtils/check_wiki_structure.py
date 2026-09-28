#!/usr/bin/env python3
"""Check the wiki's strict hierarchy under docs/.

Rules:
  * every directory (except docs/assets) has an index.md;
  * every index.md links to every other note in its directory and to the
    index.md of each immediate subdirectory;
  * note and directory names are lowercase kebab-case;
  * README.md links to the wiki home and to every top-level section index.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[1]
NAME = re.compile(r"^[a-z0-9]+(?:-[a-z0-9]+)*$")
LINK = re.compile(r"\]\(([^)#\s]+)(?:#[^)]*)?\)")
SKIPPED_DIRECTORIES = {"assets"}


def linked_paths(markdown: Path) -> set[Path]:
    base = markdown.parent
    return {
        (base / target).resolve()
        for target in LINK.findall(markdown.read_text(encoding="utf-8"))
        if "://" not in target
    }


def check(root: Path) -> list[str]:
    docs = root / "docs"
    problems: list[str] = []
    directories = [docs] + sorted(
        path
        for path in docs.rglob("*")
        if path.is_dir() and not SKIPPED_DIRECTORIES.intersection(path.relative_to(docs).parts)
    )
    for directory in directories:
        index = directory / "index.md"
        relative = directory.relative_to(root)
        if directory != docs and not NAME.match(directory.name):
            problems.append(f"{relative}: directory name is not lowercase kebab-case")
        if not index.is_file():
            problems.append(f"{relative}: missing index.md")
            continue
        links = linked_paths(index)
        for note in sorted(directory.glob("*.md")):
            if note.name == "index.md":
                continue
            if not NAME.match(note.stem):
                problems.append(f"{note.relative_to(root)}: note name is not lowercase kebab-case")
            if note.resolve() not in links:
                problems.append(f"{index.relative_to(root)}: does not link to {note.name}")
        for subdirectory in sorted(p for p in directory.iterdir() if p.is_dir()):
            if subdirectory.name in SKIPPED_DIRECTORIES and directory == docs:
                continue
            if (subdirectory / "index.md").resolve() not in links:
                problems.append(
                    f"{index.relative_to(root)}: does not link to {subdirectory.name}/index.md"
                )
    readme = root / "README.md"
    readme_links = linked_paths(readme)
    required = [docs / "index.md"] + [
        section / "index.md"
        for section in sorted(p for p in docs.iterdir() if p.is_dir())
        if section.name not in SKIPPED_DIRECTORIES
    ]
    for index in required:
        if index.resolve() not in readme_links:
            problems.append(f"README.md: does not link to {index.relative_to(root)}")
    return problems


def main() -> int:
    problems = check(REPO_ROOT)
    for problem in problems:
        print(f"wiki structure: {problem}")
    if problems:
        return 1
    print("wiki structure: OK")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
