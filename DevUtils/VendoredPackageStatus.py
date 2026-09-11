#!/usr/bin/env python3
"""Report vendored-source, local-package, and hosted-package status together.

This is an audit/reporting tool.  It never updates source pins, generated
package output, the local repository, or the hosted package repository.
"""

from __future__ import annotations

import argparse
from concurrent.futures import ThreadPoolExecutor, as_completed
from dataclasses import dataclass
import json
import subprocess
import sys
from pathlib import Path
from typing import Any

import tomllib

import UpdateChecker


ROOT = Path(__file__).resolve().parents[1]
STATE_DIRECTORY = ROOT / "upstream/state"
INVENTORY_PATH = ROOT / "out/packages/inventory.toml"
PUBLISHER_RELATIVE = Path("src/infrastructure/LinuxScripts/GenericScripts/ManageMattOSRepository.py")


class StatusError(RuntimeError):
    pass


class Colors:
    """Readable status styling; labels remain meaningful without ANSI color."""

    def __init__(self, enabled: bool):
        self.enabled = enabled

    def paint(self, code: str, text: str) -> str:
        return f"\033[{code}m{text}\033[0m" if self.enabled else text

    def heading(self, text: str) -> str:
        return self.paint("1;36", text)

    def status(self, state: str, text: str) -> str:
        if state in {"up-to-date", "verified", "published"}:
            code = "1;32"
        elif state in {"update-available", "behind", "not-published", "version-differs", "unknown"}:
            code = "1;33"
        else:
            code = "1;31"
        return self.paint(code, text)


@dataclass(frozen=True)
class ImportedState:
    component: str
    commit: str | None
    status: str
    detail: str | None


@dataclass(frozen=True)
class PackageRecord:
    name: str
    version: str
    architecture: str
    source_component: str
    sha256: str


def imported_state(root: Path, component: UpdateChecker.Component) -> ImportedState:
    path = root / "upstream/state" / f"{component.name}.toml"
    try:
        with path.open("rb") as stream:
            state = tomllib.load(stream)
    except (OSError, tomllib.TOMLDecodeError) as exc:
        return ImportedState(component.name, None, "state-error", str(exc))
    commit = state.get("imported_commit")
    if not isinstance(commit, str) or len(commit) != 40:
        return ImportedState(component.name, None, "state-error", "missing valid imported_commit")
    mismatches = []
    if state.get("component") != component.name:
        mismatches.append("component")
    if state.get("repo") != component.repository:
        mismatches.append("repo")
    if state.get("destination_path") != component.source_path:
        mismatches.append("destination_path")
    if commit != component.revision:
        mismatches.append("imported_commit")
    if mismatches:
        return ImportedState(component.name, commit, "provenance-mismatch", ", ".join(mismatches))
    return ImportedState(component.name, commit, "verified", None)


def load_inventory(path: Path) -> tuple[dict[str, list[PackageRecord]], str | None]:
    """Return local package facts grouped by their declared source component."""
    if not path.is_file():
        return {}, f"local package inventory is absent: {path}"
    try:
        with path.open("rb") as stream:
            document = tomllib.load(stream)
        entries = document["package"]
    except (OSError, KeyError, TypeError, tomllib.TOMLDecodeError) as exc:
        return {}, f"local package inventory is invalid ({path}): {exc}"
    if not isinstance(entries, list):
        return {}, f"local package inventory is invalid ({path}): package must be a list"
    grouped: dict[str, list[PackageRecord]] = {}
    for entry in entries:
        required = ("name", "version", "architecture", "source_component", "sha256")
        if not isinstance(entry, dict) or any(not isinstance(entry.get(field), str) or not entry[field] for field in required):
            return {}, f"local package inventory is invalid ({path}): malformed package entry"
        record = PackageRecord(*(str(entry[field]) for field in required))
        grouped.setdefault(record.source_component, []).append(record)
    for records in grouped.values():
        records.sort(key=lambda record: (record.name, record.architecture, record.version))
    return grouped, None


def hosted_inventory(root: Path, repository: str) -> dict[str, list[str]]:
    """Read the current hosted repository once, without publishing anything."""
    publisher = root / PUBLISHER_RELATIVE
    if not publisher.is_file():
        raise StatusError(f"vendored repository client is absent: {publisher}")
    completed = subprocess.run(
        [sys.executable, str(publisher), "--non-interactive", "--repo", repository, "list"],
        cwd=root, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, check=False,
    )
    if completed.returncode:
        detail = completed.stdout.strip() or f"exit {completed.returncode}"
        raise StatusError(f"hosted repository lookup failed: {detail}")
    result: dict[str, list[str]] = {}
    for line in completed.stdout.splitlines():
        fields = line.split("\t", 2)
        if len(fields) == 3 and fields[0] and fields[1]:
            result.setdefault(fields[0], []).append(fields[1])
    return {name: sorted(set(versions)) for name, versions in result.items()}


def newest_debian_version(versions: list[str]) -> str | None:
    newest: str | None = None
    for version in versions:
        if newest is None:
            newest = version
        elif subprocess.run(["dpkg", "--compare-versions", version, "gt", newest], check=False).returncode == 0:
            newest = version
    return newest


def component_row(component: UpdateChecker.Component, upstream: UpdateChecker.UpdateResult,
                  state: ImportedState, packages: list[PackageRecord],
                  hosted: dict[str, list[str]] | None) -> dict[str, Any]:
    package_rows = []
    for package in packages:
        published = hosted.get(package.name, []) if hosted is not None else []
        newest = newest_debian_version(published)
        if hosted is None:
            publication = "unknown"
        elif package.version in published:
            publication = "published"
        elif published:
            publication = "version-differs"
        else:
            publication = "not-published"
        package_rows.append({
            "name": package.name, "target_version": package.version,
            "architecture": package.architecture, "sha256": package.sha256,
            "published_versions": published, "newest_published": newest,
            "publication_status": publication,
        })
    return {
        "component": component.name, "repository": component.repository, "ref": component.ref,
        "upstream_status": upstream.status, "upstream_tip": upstream.latest,
        "selected_commit": component.revision, "imported_commit": state.commit,
        "provenance_status": state.status, "provenance_detail": state.detail,
        "packages": package_rows,
    }


def inspect_upstream(components: list[UpdateChecker.Component], *, exact: bool,
                     timeout: float, jobs: int) -> dict[str, UpdateChecker.UpdateResult]:
    results: dict[str, UpdateChecker.UpdateResult] = {}
    with ThreadPoolExecutor(max_workers=min(jobs, len(components) or 1)) as executor:
        futures = [executor.submit(UpdateChecker.inspect, component, exact=exact, timeout=timeout)
                   for component in components]
        for future in as_completed(futures):
            result = future.result()
            results[result.component.name] = result
    return results


def upstream_label(state: str) -> str:
    return {
        "up-to-date": "UPSTREAM CURRENT",
        "update-available": "UPSTREAM AHEAD",
        "behind": "UPSTREAM AHEAD",
        "local-ahead": "PIN AHEAD",
        "diverged": "UPSTREAM DIVERGED",
        "error": "UPSTREAM ERROR",
    }.get(state, state.upper().replace("-", " "))


def provenance_label(state: str) -> str:
    return {
        "verified": "PROVENANCE VERIFIED",
        "provenance-mismatch": "PROVENANCE MISMATCH",
        "state-error": "PROVENANCE ERROR",
    }.get(state, state.upper().replace("-", " "))


def publication_label(state: str) -> str:
    return {
        "published": "PUBLISHED",
        "not-published": "NOT PUBLISHED",
        "version-differs": "VERSION DIFFERS",
        "unknown": "PUBLISH STATE UNKNOWN",
    }.get(state, state.upper().replace("-", " "))


def render_row(row: dict[str, Any], *, verbose: bool, colors: Colors) -> list[str]:
    selected = str(row["selected_commit"])[:12]
    upstream = str(row["upstream_tip"] or "unknown")[:12]
    packages = row["packages"]
    publication_states = sorted({package["publication_status"] for package in packages})
    publication = ", ".join(publication_label(state) for state in publication_states) or "NO NATIVE PACKAGE"
    upstream_state = str(row["upstream_status"])
    provenance_state = str(row["provenance_status"])
    lines = [
        colors.heading(f"\n━━ {row['component']} ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"),
        f"  {colors.status(upstream_state, '[' + upstream_label(upstream_state) + ']'):<29} "
        f"branch {row['ref']} → {upstream}",
        f"  {colors.status(provenance_state, '[' + provenance_label(provenance_state) + ']'):<29} "
        f"selected {selected}  imported {str(row['imported_commit'] or 'unknown')[:12]}",
        f"  {colors.status(publication_states[0] if len(publication_states) == 1 else 'unknown', '[PACKAGE STATUS]'):<29} "
        f"{len(packages)} local package(s): {publication}",
    ]
    if verbose:
        for package in packages:
            published = package["newest_published"] or "not published"
            lines.append(
                f"    {colors.status(str(package['publication_status']), '[' + publication_label(str(package['publication_status'])) + ']'):<25} "
                f"{package['name']} [{package['architecture']}]\n"
                f"      target: {package['target_version']}\n"
                f"      hosted: {published}"
            )
    return lines


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--component", action="append", help="report only this vendored component; repeatable")
    parser.add_argument("--repository", default="mattos", help="hosted repository to inspect (default: mattos)")
    parser.add_argument("--no-hosted", action="store_true", help="do not contact the hosted package repository")
    parser.add_argument("--exact", action="store_true", help="calculate exact upstream commit distance using temporary Git repositories")
    parser.add_argument("--jobs", type=int, default=8, help="parallel upstream checks (default: 8)")
    parser.add_argument("--timeout", type=float, default=45.0, help="per-Git-operation timeout (default: 45)")
    parser.add_argument("--json", action="store_true", help="emit machine-readable report")
    parser.add_argument("--verbose", action="store_true", help="include one row for every locally built package")
    parser.add_argument("--color", choices=("always", "never"), default="always",
                        help="color terminal status labels (default: always)")
    return parser.parse_args(argv)


def main(argv: list[str] | None = None) -> int:
    args = parse_args(argv)
    if args.jobs < 1 or args.timeout <= 0:
        raise SystemExit("--jobs must be at least 1 and --timeout must be greater than zero")
    components = UpdateChecker.load_components(ROOT / "upstream/sources.toml")
    requested = set(args.component or [])
    known = {component.name for component in components}
    unknown = requested - known
    if unknown:
        raise SystemExit("unknown component(s): " + ", ".join(sorted(unknown)))
    if requested:
        components = [component for component in components if component.name in requested]
    local, local_error = load_inventory(ROOT / "out/packages/inventory.toml")
    hosted: dict[str, list[str]] | None = None
    hosted_error: str | None = None
    if not args.no_hosted:
        try:
            hosted = hosted_inventory(ROOT, args.repository)
        except StatusError as exc:
            hosted_error = str(exc)
    upstream = inspect_upstream(components, exact=args.exact, timeout=args.timeout, jobs=args.jobs)
    rows = [component_row(component, upstream[component.name], imported_state(ROOT, component),
                          local.get(component.name, []), hosted)
            for component in components]
    report = {
        "format": 1, "repository": args.repository, "local_inventory_error": local_error,
        "hosted_inventory_error": hosted_error, "components": rows,
    }
    if args.json:
        print(json.dumps(report, indent=2, sort_keys=True))
    else:
        colors = Colors(args.color == "always")
        print(colors.heading("MattOS vendored source and package status"))
        print("Each component shows: upstream branch state • MattOS source pin/provenance • local .deb → hosted repository")
        print("Legend: green=verified/current/published; yellow=update or publication attention; red=error/mismatch.")
        if local_error:
            print(colors.status("error", f"[LOCAL INVENTORY ERROR] {local_error}"))
        if hosted_error:
            print(colors.status("error", f"[HOSTED INVENTORY ERROR] {hosted_error}"))
        elif hosted is not None:
            print(colors.status("published", f"[HOSTED INVENTORY LOADED] {args.repository}: {len(hosted)} package name(s), queried once."))
        for row in rows:
            for line in render_row(row, verbose=args.verbose, colors=colors):
                print(line)
    return 1 if local_error or hosted_error or any(
        row["upstream_status"] == "error" or row["provenance_status"] != "verified" for row in rows
    ) else 0


if __name__ == "__main__":
    raise SystemExit(main())
