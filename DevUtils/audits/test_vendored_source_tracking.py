#!/usr/bin/env python3
"""Fast, network-free check that vendored source is fully tracked by Git.

Nested upstream ``.gitignore`` files remain active inside vendored trees, so a
file upstream tracks can exist locally yet be silently missing from a fresh
clone (and from builds in a fresh checkout). Builds
never write into vendored trees, so any ignored path there is either an
untracked upstream file or residue; both must be resolved. Gitlinks in the
MattOS index likewise check out as empty directories.
"""

from __future__ import annotations

import hashlib
import sys
import tomllib
import importlib.util
from pathlib import Path
import subprocess
import unittest


ROOT = Path(__file__).resolve().parents[2]
MODULE_PATH = Path(__file__).with_name("test_imported_source_immutability.py")
SPEC = importlib.util.spec_from_file_location("imported_source_immutability", MODULE_PATH)
assert SPEC and SPEC.loader
IMMUTABILITY = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(IMMUTABILITY)

# Interpreter caches from running vendored helper scripts in place are local
# noise, not source; they are never copied into build mirrors.
WORKTREE = "--worktree" in sys.argv
if WORKTREE:
    sys.argv.remove("--worktree")

TOLERATED_PARTS = frozenset({"__pycache__"})


def untracked_vendored_paths(repository: Path) -> list[str]:
    return sorted(
        path
        for path in IMMUTABILITY.ignored_untracked_paths(repository)
        if not TOLERATED_PARTS.intersection(Path(path).parts)
    )


def index_gitlinks(repository: Path) -> list[str]:
    raw = subprocess.run(
        ["git", "ls-files", "-s", "-z"],
        cwd=repository,
        stdout=subprocess.PIPE,
        check=True,
    ).stdout
    return sorted(
        record.split(b"\t", 1)[1].decode("utf-8", "surrogateescape")
        for record in raw.split(b"\0")
        if record.startswith(b"160000 ")
    )


def incomplete_imports(repository: Path, *, worktree: bool = False) -> list[str]:
    """Compare checkout contents to every recorded import, without a network.

    The index view catches files accidentally omitted from a commit even if
    ignored copies still exist locally. The worktree view validates pending
    repairs without staging them; it includes ignored files and deletions.
    Separately pinned nested components belong only to their longest root.
    """
    components = tomllib.loads((repository / 'upstream/sources.toml').read_text())['component']
    roots = {component['path']: component for component in components}
    records = {component['name']: {} for component in components}

    def owner(path):
        for parent in Path(path).parents:
            component = roots.get(parent.as_posix())
            if component is not None:
                return component, Path(path).relative_to(parent).as_posix()
        return None

    raw = subprocess.check_output(['git', 'ls-files', '-s', '-z', '--', *roots], cwd=repository)
    for record in raw.split(b'\0'):
        if not record:
            continue
        metadata, path = record.split(b'\t', 1)
        mode, oid, stage = metadata.decode().split()
        found = owner(path.decode('utf-8', 'surrogateescape'))
        if found and mode != '160000':
            component, relative = found
            records[component['name']][relative] = (mode, oid)
    if worktree:
        changed = subprocess.check_output(['git', 'ls-files', '-m', '-d', '-o', '-z', '--', *roots], cwd=repository)
        for raw_path in set(changed.split(b'\0')) - {b''}:
            path = raw_path.decode('utf-8', 'surrogateescape')
            if TOLERATED_PARTS.intersection(Path(path).parts):
                continue
            found = owner(path)
            if not found:
                continue
            component, relative = found
            target = repository / path
            if not target.exists() and not target.is_symlink():
                records[component['name']].pop(relative, None)
                continue
            if target.is_symlink():
                import os
                payload = os.fsencode(os.readlink(target))
                mode = '120000'
            elif target.is_file():
                payload = target.read_bytes()
                mode = '100755' if target.stat().st_mode & 0o111 else '100644'
            else:
                continue
            oid = hashlib.sha1(f'blob {len(payload)}\0'.encode() + payload, usedforsecurity=False).hexdigest()
            records[component['name']][relative] = (mode, oid)
    failures = []
    for component in components:
        state = tomllib.loads((repository / 'upstream/state' / (component['name'] + '.toml')).read_text())
        digest = hashlib.sha256()
        for path, (mode, oid) in sorted(records[component['name']].items()):
            digest.update(f'{mode} blob {oid}\t{path}\0'.encode('utf-8', 'surrogateescape'))
        if digest.hexdigest() != state['imported_tree_digest']:
            failures.append(component['name'])
    return failures


class VendoredSourceTrackingTests(unittest.TestCase):
    def test_no_ignored_untracked_paths_in_vendored_trees(self) -> None:
        paths = untracked_vendored_paths(ROOT)
        if WORKTREE:
            # Exact import digests below validate ignored pending upstream files.
            # Attribute residue is forbidden even when Git hides it.
            paths = [path for path in paths if Path(path).name == ".gitattributes"]
        self.assertEqual(
            paths,
            [],
            "vendored paths are present but ignored by Git; force-add upstream "
            "files (git add -f) or remove residue:\n" + "\n".join(paths[:50]),
        )

    def test_every_import_is_complete(self) -> None:
        self.assertEqual(incomplete_imports(ROOT, worktree=WORKTREE), [],
                         "imported source differs from its recorded tree; restore missing files "
                         "and force-add ignored upstream paths before committing")

    def test_index_contains_no_gitlinks(self) -> None:
        self.assertEqual(index_gitlinks(ROOT), [])

    def test_root_gitignore_does_not_hide_license_files(self) -> None:
        completed = subprocess.run(
            ["git", "check-ignore", "-q", "--no-index", "src/example/LICENSE"],
            cwd=ROOT,
            check=False,
        )
        self.assertEqual(completed.returncode, 1, "root .gitignore ignores LICENSE files")


if __name__ == "__main__":
    unittest.main()
