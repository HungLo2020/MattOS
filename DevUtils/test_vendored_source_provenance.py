#!/usr/bin/env python3
"""Verify every MattOS vendored source tree against its immutable upstream pin.

The audit fetches commit/tree metadata only, hashes local files as Git blobs, and
compares paths, contents, executable bits, symlink targets, and gitlinks. It does
not modify imported sources or the MattOS Git index. Ignored paths absent from
the upstream tree are reported as retained generated residue but are not treated
as provenance inputs.

Use ``--emit-state-values`` to print the authoritative upstream tree object and
the canonical imported-tree SHA-256 used by schema-v2 state records.
"""

from __future__ import annotations

import argparse
import concurrent.futures
import hashlib
import os
from pathlib import Path
import re
import shutil
import stat
import subprocess
import sys
import tempfile
import tomllib


ROOT = Path(__file__).resolve().parents[1]
SOURCES_PATH = ROOT / "upstream/sources.toml"
STATE_DIR = ROOT / "upstream/state"
GITLINK_POLICY_PATH = ROOT / "upstream/policies/gitlinks.toml"
MIRROR_POLICY_PATH = ROOT / "upstream/policies/verification-mirrors.toml"
LINUXSCRIPTS_POLICY_PATH = ROOT / "upstream/policies/linuxscripts.toml"
RELEASE_ARCHIVE_POLICY_PATH = ROOT / "upstream/policies/release-archives.toml"
CACHE_ROOT = Path(os.environ.get("MATTOS_PROVENANCE_CACHE", "/tmp/mattos-vendored-source-audit-cache"))
REVISION_RE = re.compile(r"^[0-9a-f]{40}$")
EXPECTED_LINUX_COMMIT = "8ba098e6b6ff0db8edf28528d1552be261af30d4"
IMPORTED_DIGEST_ALGORITHM = "sha256-git-ls-tree-no-gitlinks-v1"
SELECTED_IMPORTED_DIGEST_ALGORITHM = "sha256-selected-git-ls-tree-no-gitlinks-v1"


class AuditFailure(RuntimeError):
    pass


def run(command: list[str], *, cwd: Path = ROOT, input_bytes: bytes | None = None) -> bytes:
    completed = subprocess.run(
        command,
        cwd=cwd,
        input=input_bytes,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    if completed.returncode != 0:
        detail = completed.stderr.decode("utf-8", "replace").strip()
        raise AuditFailure(f"command failed ({' '.join(command)}): {detail}")
    return completed.stdout


def load_toml(path: Path) -> dict:
    with path.open("rb") as stream:
        return tomllib.load(stream)


def git_blob_oid(payload: bytes) -> str:
    header = f"blob {len(payload)}\0".encode()
    return hashlib.sha1(header + payload, usedforsecurity=False).hexdigest()


def fetch_tree(component: dict, mirrors: dict[str, list[str]]) -> tuple[str, list[tuple[str, str, str, str]]]:
    name = component["name"]
    revision = component["revision"]
    cache = CACHE_ROOT / f"{name}.git"
    cache.parent.mkdir(parents=True, exist_ok=True)
    if not cache.exists():
        run(["git", "init", "--bare", "-q", str(cache)], cwd=ROOT)

    urls = [component["repo"], *mirrors.get(name, [])]
    if subprocess.run(
        ["git", "--git-dir", str(cache), "cat-file", "-e", f"{revision}^{{commit}}"],
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        check=False,
    ).returncode != 0:
        failures: list[str] = []
        for url in urls:
            completed = subprocess.run(
                [
                    "git",
                    "--git-dir",
                    str(cache),
                    "fetch",
                    "--quiet",
                    "--no-tags",
                    "--depth=1",
                    "--filter=blob:none",
                    url,
                    revision,
                ],
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                check=False,
            )
            if completed.returncode == 0:
                break
            failures.append(f"{url}: {completed.stderr.decode('utf-8', 'replace').strip()}")
        else:
            raise AuditFailure(f"{name}: unable to fetch exact commit {revision}: {' | '.join(failures)}")

    tree = run(
        ["git", "--git-dir", str(cache), "rev-parse", f"{revision}^{{tree}}"]
    ).decode().strip()
    raw = run(["git", "--git-dir", str(cache), "ls-tree", "-rz", revision])
    entries: list[tuple[str, str, str, str]] = []
    for record in raw.split(b"\0"):
        if not record:
            continue
        metadata, raw_path = record.split(b"\t", 1)
        mode, object_type, oid = metadata.decode("ascii").split(" ")
        entries.append((mode, object_type, oid, raw_path.decode("utf-8", "surrogateescape")))
    return tree, entries


def imported_digest(entries: list[tuple[str, str, str, str]]) -> str:
    digest = hashlib.sha256()
    for mode, object_type, oid, path in entries:
        if mode == "160000":
            continue
        digest.update(f"{mode} {object_type} {oid}\t{path}\0".encode("utf-8", "surrogateescape"))
    return digest.hexdigest()


def source_selection_retains(policy: dict | None, path: str) -> bool:
    if policy is None or not path.startswith("arch/"):
        return True
    parts = path.split("/")
    if len(parts) < 3:
        return policy.get("retain_arch_root_files") is True
    arch_relative = "/".join(parts[1:])
    if arch_relative in policy.get("retained_arch_paths", []):
        return True
    if parts[1] not in policy["retained_architectures"]:
        return False
    if parts[1] != "x86":
        return True
    return "/".join(parts[2:]) not in policy.get("x86_excluded_paths", [])


def load_source_selection_policy(component: dict, state: dict) -> tuple[dict | None, list[str]]:
    name = component["name"]
    policy_name = component.get("source_selection_policy", "none")
    expected_sha256 = component.get("source_selection_policy_sha256", "none")
    failures: list[str] = []
    if state.get("source_selection_policy", "none") != policy_name:
        failures.append(f"{name}: state source_selection_policy does not match sources.toml")
    if state.get("source_selection_policy_sha256", "none") != expected_sha256:
        failures.append(f"{name}: state source_selection_policy_sha256 does not match sources.toml")
    if policy_name == "none":
        if expected_sha256 != "none":
            failures.append(f"{name}: source-selection digest exists without a policy")
        return None, failures

    policy_path = ROOT / policy_name
    if not policy_path.is_file():
        failures.append(f"{name}: source-selection policy is missing: {policy_name}")
        return None, failures
    payload = policy_path.read_bytes()
    actual_sha256 = hashlib.sha256(payload).hexdigest()
    if actual_sha256 != expected_sha256:
        failures.append(f"{name}: source-selection policy checksum mismatch")
    policy = tomllib.loads(payload.decode())
    if policy.get("schema_version") != 1:
        failures.append(f"{name}: unsupported source-selection policy schema")
    if policy.get("component") != name:
        failures.append(f"{name}: source-selection policy component mismatch")
    if policy.get("upstream_commit") != component.get("revision"):
        failures.append(f"{name}: source-selection policy commit mismatch")
    if policy.get("scope") != "arch":
        failures.append(f"{name}: source-selection policy exceeds architecture scope")
    if policy.get("retain_arch_root_files") is not True:
        failures.append(f"{name}: shared arch root files are not retained")
    if policy.get("retained_architectures") != ["x86", "arm64", "riscv", "um"]:
        failures.append(f"{name}: source-selection retained architecture set is unsupported")
    if policy.get("retained_arch_paths", []) != [
        "arm/crypto/Kconfig",
        "powerpc/crypto/Kconfig",
        "s390/crypto/Kconfig",
        "sparc/crypto/Kconfig",
    ]:
        failures.append(f"{name}: source-selection retained architecture paths are unsupported")
    return policy, failures


def collect_local_leaf_paths(root: Path) -> set[str]:
    leaves: set[str] = set()
    for current, directories, files in os.walk(root, followlinks=False):
        current_path = Path(current)
        kept_directories: list[str] = []
        for name in directories:
            candidate = current_path / name
            relative = candidate.relative_to(root).as_posix()
            if name == ".git":
                leaves.add(relative + "/")
            elif candidate.is_symlink():
                leaves.add(relative)
            else:
                kept_directories.append(name)
        directories[:] = kept_directories
        for name in files:
            leaves.add((current_path / name).relative_to(root).as_posix())
    return leaves


def ignored_repository_paths(paths: list[Path]) -> set[str]:
    if not paths:
        return set()
    raw = b"".join(os.fsencode(path.as_posix()) + b"\0" for path in paths)
    completed = subprocess.run(
        ["git", "check-ignore", "-z", "--stdin"],
        cwd=ROOT,
        input=raw,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    if completed.returncode not in (0, 1):
        raise AuditFailure(completed.stderr.decode("utf-8", "replace"))
    return {
        os.fsdecode(value)
        for value in completed.stdout.split(b"\0")
        if value
    }


def symlink_escapes(component_root: Path, path: str, target: str) -> bool:
    if os.path.isabs(target):
        return True
    joined = os.path.normpath(os.path.join(os.path.dirname(path), target))
    return joined == ".." or joined.startswith("../")


def filtered_worktree_blob_oids(component: dict, paths: list[str]) -> dict[str, str]:
    if not paths:
        return {}
    repository_paths = [f"{component['path']}/{path}" for path in paths]
    if any("\n" in path for path in repository_paths):
        raise AuditFailure(f"{component['name']}: newline in source path is unsupported")
    output = run(
        ["git", "hash-object", "--stdin-paths"],
        input_bytes=("\n".join(repository_paths) + "\n").encode("utf-8", "surrogateescape"),
    )
    object_ids = output.decode("ascii").splitlines()
    if len(object_ids) != len(paths):
        raise AuditFailure(f"{component['name']}: Git did not hash every regular source path")
    return dict(zip(paths, object_ids, strict=True))


def verify_component_tree(
    component: dict,
    state: dict,
    tree: str,
    entries: list[tuple[str, str, str, str]],
    gitlink_policies: dict[tuple[str, str], dict],
    source_selection: dict | None,
) -> tuple[int, list[str]]:
    name = component["name"]
    source_root = ROOT / component["path"]
    failures: list[str] = []
    expected_non_gitlinks = {path for mode, _, _, path in entries if mode != "160000"}
    regular_paths = [
        path
        for mode, _, _, path in entries
        if mode in ("100644", "100755") and (source_root / path).is_file()
    ]
    regular_oids = filtered_worktree_blob_oids(component, regular_paths)

    for mode, object_type, oid, path in entries:
        if mode == "160000":
            policy = gitlink_policies.get((name, path))
            if policy is None:
                failures.append(f"unmapped gitlink {path} -> {oid}")
            elif policy.get("upstream_commit") != oid:
                failures.append(
                    f"gitlink policy mismatch {path}: expected {oid}, records {policy.get('upstream_commit')}"
                )
            continue

        local = source_root / path
        try:
            metadata = local.lstat()
        except FileNotFoundError:
            failures.append(f"missing upstream path {path}")
            continue

        if mode == "120000":
            if not stat.S_ISLNK(metadata.st_mode):
                failures.append(f"type mismatch {path}: expected symlink")
                continue
            target = os.readlink(local)
            payload = os.fsencode(target)
            if symlink_escapes(source_root, path, target):
                failures.append(f"upstream symlink escapes component tree: {path} -> {target}")
        else:
            if not stat.S_ISREG(metadata.st_mode):
                failures.append(f"type mismatch {path}: expected regular file")
                continue
            with local.open("rb") as stream:
                prefix = stream.read(128)
            actual_mode = "100755" if metadata.st_mode & 0o111 else "100644"
            if actual_mode != mode:
                failures.append(f"mode mismatch {path}: expected {mode}, got {actual_mode}")
            if prefix.startswith(b"version https://git-lfs.github.com/spec/v1\n"):
                failures.append(f"Git LFS pointer present: {path}")
            actual_oid = regular_oids[path]

        if mode == "120000":
            actual_oid = git_blob_oid(payload)
        if actual_oid != oid:
            failures.append(f"blob mismatch {path}: expected {oid}, got {actual_oid}")

    local_paths = collect_local_leaf_paths(source_root)
    nested_git = sorted(path for path in local_paths if path.endswith(".git/"))
    failures.extend(f"nested Git directory {path}" for path in nested_git)
    stale_excluded = sorted(
        path
        for path in local_paths
        if not path.endswith(".git/") and not source_selection_retains(source_selection, path)
    )
    failures.extend(f"stale source-selection-excluded path {path}" for path in stale_excluded)
    extra_paths = sorted(local_paths - expected_non_gitlinks - set(nested_git))
    repository_extras = [Path(component["path"]) / path for path in extra_paths]
    ignored = ignored_repository_paths(repository_extras)
    unexplained_extras = [
        path
        for path, repository_path in zip(extra_paths, repository_extras, strict=True)
        if repository_path.as_posix() not in ignored
    ]
    failures.extend(f"extra local file {path}" for path in unexplained_extras)

    if state.get("upstream_tree") != tree:
        failures.append(f"state upstream_tree is {state.get('upstream_tree')!r}, expected {tree}")
    digest = imported_digest(entries)
    if state.get("imported_tree_digest") != digest:
        failures.append(
            f"state imported_tree_digest is {state.get('imported_tree_digest')!r}, expected {digest}"
        )
    expected_algorithm = (
        SELECTED_IMPORTED_DIGEST_ALGORITHM if source_selection is not None else IMPORTED_DIGEST_ALGORITHM
    )
    if state.get("imported_tree_digest_algorithm") != expected_algorithm:
        failures.append("state imported-tree digest algorithm is missing or unsupported")

    return len(ignored), failures


def load_gitlink_policies() -> tuple[dict[tuple[str, str], dict], dict[str, list[dict]]]:
    document = load_toml(GITLINK_POLICY_PATH)
    by_path: dict[tuple[str, str], dict] = {}
    by_component: dict[str, list[dict]] = {}
    for component in document.get("component", []):
        name = component["name"]
        policies = component.get("gitlink", [])
        by_component[name] = policies
        for policy in policies:
            by_path[(name, policy["path"])] = policy
    return by_path, by_component


def verify_gitlink_replacements(
    policies_by_component: dict[str, list[dict]], components: dict[str, dict]
) -> list[str]:
    failures: list[str] = []
    for owner, policies in policies_by_component.items():
        for policy in policies:
            action = policy.get("action")
            if action == "exclude":
                if not policy.get("reason"):
                    failures.append(f"{owner}:{policy['path']} exclusion lacks a reason")
                continue
            if action != "replacement":
                failures.append(f"{owner}:{policy['path']} has unsupported action {action!r}")
                continue
            replacement = components.get(policy.get("replacement_component"))
            if replacement is None:
                failures.append(f"{owner}:{policy['path']} replacement component is undefined")
                continue
            if replacement["path"] != policy.get("replacement_path"):
                failures.append(f"{owner}:{policy['path']} replacement path does not match sources.toml")
            if replacement["revision"] != policy.get("replacement_commit"):
                failures.append(f"{owner}:{policy['path']} replacement commit does not match sources.toml")
            exact = policy.get("upstream_commit") == policy.get("replacement_commit")
            if exact != policy.get("exact_gitlink_match"):
                failures.append(f"{owner}:{policy['path']} exact_gitlink_match is false metadata")
            if not exact and not policy.get("reason"):
                failures.append(f"{owner}:{policy['path']} version override lacks a reason")
    return failures


def patch_input_paths(patch: bytes) -> set[str]:
    paths: set[str] = set()
    for line in patch.splitlines():
        if line.startswith(b"--- a/"):
            paths.add(line[6:].decode("utf-8", "surrogateescape"))
    return paths


def verify_patch_manifest(
    component: dict,
    state: dict,
    tree: str,
    policies_by_component: dict[str, list[dict]],
    components: dict[str, dict],
) -> list[str]:
    manifest_name = state.get("patch_manifest")
    if not manifest_name or manifest_name == "none":
        return []
    failures: list[str] = []
    manifest_path = ROOT / manifest_name
    manifest_payload = manifest_path.read_bytes()
    manifest_sha256 = hashlib.sha256(manifest_payload).hexdigest()
    if manifest_sha256 != state.get("patch_manifest_sha256"):
        failures.append(f"{component['name']}: patch manifest checksum mismatch")
    manifest = load_toml(manifest_path)
    if manifest.get("component") != component["name"]:
        failures.append(f"{component['name']}: patch manifest component mismatch")
    if manifest.get("upstream_commit") != component["revision"]:
        failures.append(f"{component['name']}: patch manifest commit mismatch")
    if manifest.get("upstream_tree") != tree:
        failures.append(f"{component['name']}: patch manifest tree mismatch")
    if manifest.get("application") != "output-mirror-only":
        failures.append(f"{component['name']}: patches are not restricted to output mirrors")

    with tempfile.TemporaryDirectory(prefix=f"mattos-patch-{component['name']}-") as raw_temp:
        mirror = Path(raw_temp)
        for record in manifest.get("patch", []):
            patch_path = ROOT / record["path"]
            payload = patch_path.read_bytes()
            actual = hashlib.sha256(payload).hexdigest()
            if actual != record.get("sha256"):
                failures.append(
                    f"{component['name']}: patch checksum mismatch for {record['path']}"
                )
                continue
            for relative in patch_input_paths(payload):
                source = ROOT / component["path"] / relative
                if not source.exists() and not source.is_symlink():
                    relative_path = Path(relative)
                    for policy in policies_by_component.get(component["name"], []):
                        if policy.get("action") != "replacement":
                            continue
                        try:
                            replacement_relative = relative_path.relative_to(policy["path"])
                        except ValueError:
                            continue
                        replacement = components.get(policy.get("replacement_component"))
                        if replacement is not None:
                            source = ROOT / replacement["path"] / replacement_relative
                        break
                if not source.exists() and not source.is_symlink():
                    failures.append(
                        f"{component['name']}: patch input {relative} is absent from the pinned tree and declared replacements"
                    )
                    continue
                destination = mirror / relative
                destination.parent.mkdir(parents=True, exist_ok=True)
                if source.is_symlink():
                    destination.symlink_to(os.readlink(source))
                else:
                    shutil.copy2(source, destination)
            completed = subprocess.run(
                ["git", "apply", "--check", "--whitespace=error-all", str(patch_path)],
                cwd=mirror,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                check=False,
            )
            if completed.returncode != 0:
                failures.append(
                    f"{component['name']}: patch does not apply to pinned tree: "
                    + completed.stderr.decode("utf-8", "replace").strip()
                )
    return failures


def verify_linuxscripts() -> list[str]:
    policy = load_toml(LINUXSCRIPTS_POLICY_PATH)
    authoritative = ROOT / policy["authoritative_path"]
    actual = hashlib.sha256(authoritative.read_bytes()).hexdigest()
    failures: list[str] = []
    if actual != policy["sha256"]:
        failures.append(f"linuxscripts: ManageMattOSRepository.py checksum is {actual}")
    if (ROOT / "src/infrastructure/LinuxScripts/.git").exists():
        failures.append("linuxscripts: nested .git exists")
    return failures


def verify_release_archive_policy(components: dict[str, dict]) -> list[str]:
    document = load_toml(RELEASE_ARCHIVE_POLICY_PATH)
    builder = (ROOT / "src/tools/mattos-build/src/main.rs").read_text()
    failures: list[str] = []
    for archive in document.get("archive", []):
        name = archive["component"]
        component = components.get(name)
        if component is None:
            failures.append(f"release archive references unknown component {name}")
            continue
        if component.get("revision") != archive.get("source_commit"):
            failures.append(f"{name}: release archive source commit does not match sources.toml")
        if component.get("branch") != archive.get("source_tag"):
            failures.append(f"{name}: release archive tag does not match sources.toml")
        if archive.get("version") not in archive.get("url", ""):
            failures.append(f"{name}: release archive URL does not contain its pinned version")
        if not re.fullmatch(r"[0-9a-f]{64}", archive.get("sha256", "")):
            failures.append(f"{name}: release archive SHA-256 is invalid")
        if archive.get("staging_policy") != "output-mirror-only":
            failures.append(f"{name}: release archive is not restricted to output-owned staging")
        if archive.get("url") not in builder or archive.get("sha256") not in builder:
            failures.append(f"{name}: builder does not enforce the recorded archive URL and checksum")
    return failures


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--emit-state-values", action="store_true")
    parser.add_argument("--jobs", type=int, default=6)
    args = parser.parse_args()

    source_document = load_toml(SOURCES_PATH)
    component_list = source_document.get("component", [])
    components = {component["name"]: component for component in component_list}
    failures: list[str] = []
    if len(components) != 53 or len(component_list) != 53:
        failures.append(f"sources.toml declares {len(component_list)} components, expected 53 unique components")
    for component in component_list:
        revision = component.get("revision", "")
        if not REVISION_RE.fullmatch(revision):
            failures.append(f"{component['name']}: revision is not an exact 40-hex commit")
    if components.get("linux", {}).get("revision") != EXPECTED_LINUX_COMMIT:
        failures.append("linux: provenance is not pinned to the required upstream commit")

    mirror_document = load_toml(MIRROR_POLICY_PATH)
    mirrors: dict[str, list[str]] = {}
    for mirror in mirror_document.get("mirror", []):
        mirrors.setdefault(mirror["component"], []).append(mirror["url"])

    fetched: dict[str, tuple[str, list[tuple[str, str, str, str]]]] = {}
    with concurrent.futures.ThreadPoolExecutor(max_workers=max(1, args.jobs)) as executor:
        pending = {
            executor.submit(fetch_tree, component, mirrors): component["name"]
            for component in component_list
            if REVISION_RE.fullmatch(component.get("revision", ""))
        }
        for future in concurrent.futures.as_completed(pending):
            name = pending[future]
            try:
                fetched[name] = future.result()
            except Exception as error:  # audit must collect every component failure
                failures.append(f"{name}: {error}")

    if args.emit_state_values:
        for name in sorted(fetched):
            tree, entries = fetched[name]
            component = components[name]
            state_path = STATE_DIR / f"{name}.toml"
            state = load_toml(state_path) if state_path.is_file() else {}
            source_selection, selection_failures = load_source_selection_policy(component, state)
            failures.extend(selection_failures)
            entries = [
                entry for entry in entries if source_selection_retains(source_selection, entry[3])
            ]
            print(f"{name}\t{tree}\t{imported_digest(entries)}")
        if failures:
            print("\n".join(f"ERROR: {failure}" for failure in failures), file=sys.stderr)
            return 1
        return 0

    gitlink_by_path, gitlinks_by_component = load_gitlink_policies()
    failures.extend(verify_gitlink_replacements(gitlinks_by_component, components))
    ignored_total = 0
    verified = 0
    for component in component_list:
        name = component["name"]
        if name not in fetched:
            continue
        state_path = STATE_DIR / f"{name}.toml"
        if not state_path.is_file():
            failures.append(f"{name}: state record is missing")
            continue
        state = load_toml(state_path)
        source_selection, selection_failures = load_source_selection_policy(component, state)
        failures.extend(selection_failures)
        for field, source_field in (("component", "name"), ("repo", "repo"), ("branch", "branch")):
            if state.get(field) != component.get(source_field):
                failures.append(f"{name}: state {field} does not match sources.toml")
        if state.get("imported_commit") != component.get("revision"):
            failures.append(f"{name}: state commit does not match immutable revision")
        if state.get("destination_path") != component.get("path"):
            failures.append(f"{name}: state destination does not match sources.toml")
        if state.get("sync_method") != component.get("sync"):
            failures.append(f"{name}: state sync method does not match sources.toml")
        for field in (
            "source_selection_policy",
            "source_selection_policy_sha256",
            "intentional_omission_policy",
            "gitlink_policy",
            "patch_manifest",
            "patch_manifest_sha256",
        ):
            if state.get(field, "none") != component.get(field, "none"):
                failures.append(f"{name}: state {field} does not match sources.toml")
        if state.get("schema_version") != 2:
            failures.append(f"{name}: state schema_version is not 2")
        tree, entries = fetched[name]
        entries = [
            entry for entry in entries if source_selection_retains(source_selection, entry[3])
        ]
        ignored_count, tree_failures = verify_component_tree(
            component, state, tree, entries, gitlink_by_path, source_selection
        )
        ignored_total += ignored_count
        failures.extend(f"{name}: {failure}" for failure in tree_failures)
        failures.extend(
            verify_patch_manifest(
                component,
                state,
                tree,
                gitlinks_by_component,
                components,
            )
        )
        verified += 1

    failures.extend(verify_linuxscripts())
    failures.extend(verify_release_archive_policy(components))
    mapped_gitlinks = set(gitlink_by_path)
    observed_gitlinks = {
        (name, path)
        for name, (_, entries) in fetched.items()
        for mode, _, _, path in entries
        if mode == "160000"
    }
    for name, path in sorted(mapped_gitlinks - observed_gitlinks):
        failures.append(f"{name}: policy maps nonexistent gitlink {path}")

    print(f"components verified: {verified}/47")
    print(f"ignored generated-residue paths retained outside provenance: {ignored_total}")
    print(f"gitlinks verified against explicit policy: {len(observed_gitlinks)}")
    print("unpinned components: 0" if not any("revision" in f for f in failures) else "unpinned components: FAILED")
    if failures:
        print(f"vendored-source provenance audit: FAILED ({len(failures)} findings)")
        for failure in failures:
            print(f"- {failure}")
        return 1
    print("missing upstream files: 0")
    print("unexplained source differences: 0")
    print("vendored-source provenance audit: PASS")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
