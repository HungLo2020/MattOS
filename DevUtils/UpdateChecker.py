#!/usr/bin/env python3
"""Report whether pinned MattOS source components have upstream updates.

This is intentionally read-only with respect to the MattOS checkout. Git
history needed for commit-distance calculation is fetched into temporary bare
repositories, never into a component source directory or the repository's
Git directory.
"""

from __future__ import annotations

import argparse
from concurrent.futures import ThreadPoolExecutor, as_completed
from dataclasses import dataclass
import subprocess
import tempfile
import sys
from pathlib import Path

import tomllib


ROOT = Path(__file__).resolve().parents[1]
SOURCES = ROOT / "upstream" / "sources.toml"


@dataclass(frozen=True)
class Component:
    name: str
    repository: str
    ref: str
    revision: str
    source_path: str


@dataclass(frozen=True)
class UpdateResult:
    component: Component
    status: str
    behind: int | None
    current: str
    latest: str | None
    detail: str | None = None


def load_components(path: Path = SOURCES) -> list[Component]:
    with path.open("rb") as stream:
        document = tomllib.load(stream)

    components: list[Component] = []
    for item in document.get("component", []):
        if item.get("sync") != "copy":
            continue
        fields = ("name", "repo", "branch", "revision", "path")
        missing = [field for field in fields if not item.get(field)]
        if missing:
            raise ValueError(
                f"component entry is missing {', '.join(missing)}: {item!r}"
            )
        revision = str(item["revision"])
        if len(revision) != 40 or any(char not in "0123456789abcdef" for char in revision.lower()):
            raise ValueError(f"{item['name']}: revision is not a 40-hex commit: {revision}")
        components.append(
            Component(
                name=str(item["name"]),
                repository=str(item["repo"]),
                ref=str(item["branch"]),
                revision=revision,
                source_path=str(item["path"]),
            )
        )
    return sorted(components, key=lambda component: component.name)


def git(
    command: list[str], *, cwd: Path | None = None, timeout: float
) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        ["git", *command],
        cwd=cwd,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
        timeout=timeout,
    )


def remote_tip(component: Component, timeout: float) -> str:
    completed = git(
        ["ls-remote", "--exit-code", component.repository, component.ref],
        timeout=timeout,
    )
    if completed.returncode != 0:
        detail = completed.stderr.strip() or completed.stdout.strip() or "ref not found"
        raise RuntimeError(f"cannot resolve {component.ref!r}: {detail}")
    matches = [line.split()[0] for line in completed.stdout.splitlines() if line.split()]
    if not matches:
        raise RuntimeError(f"no commit returned for ref {component.ref!r}")
    return matches[0]


def commit_distance(
    component: Component, latest: str, timeout: float
) -> tuple[str, int | None, str | None]:
    with tempfile.TemporaryDirectory(prefix="mattos-update-check-") as temporary:
        repository = Path(temporary)
        initialized = git(["init", "--bare", "-q"], cwd=repository, timeout=timeout)
        if initialized.returncode != 0:
            raise RuntimeError(initialized.stderr.strip() or "git init failed")

        # Exact mode is deliberately opt-in. Start shallow and deepen only as
        # needed; most recently-pinned components are close to their ref tip.
        fetch = git(
            [
                "fetch",
                "--filter=blob:none",
                "--depth=64",
                "--no-tags",
                "--quiet",
                component.repository,
                component.ref,
            ],
            cwd=repository,
            timeout=timeout,
        )
        if fetch.returncode != 0:
            fetch = git(
                ["fetch", "--depth=64", "--no-tags", "--quiet", component.repository, component.ref],
                cwd=repository,
                timeout=timeout,
            )
        if fetch.returncode != 0:
            raise RuntimeError(fetch.stderr.strip() or "unable to fetch upstream ref")

        pinned_exists = git(
            ["cat-file", "-e", f"{component.revision}^{{commit}}"],
            cwd=repository,
            timeout=timeout,
        )
        if pinned_exists.returncode != 0:
            depth = 128
            while pinned_exists.returncode != 0 and depth <= 8192:
                deepened = git(
                    [
                        "fetch",
                        "--deepen",
                        str(depth),
                        "--filter=blob:none",
                        "--no-tags",
                        "--quiet",
                        component.repository,
                        component.ref,
                    ],
                    cwd=repository,
                    timeout=timeout,
                )
                if deepened.returncode != 0:
                    break
                pinned_exists = git(
                    ["cat-file", "-e", f"{component.revision}^{{commit}}"],
                    cwd=repository,
                    timeout=timeout,
                )
                depth *= 2
            if pinned_exists.returncode != 0:
                unshallow = git(
                    ["fetch", "--unshallow", "--no-tags", "--quiet", component.repository, component.ref],
                    cwd=repository,
                    timeout=timeout,
                )
                if unshallow.returncode != 0:
                    raise RuntimeError(unshallow.stderr.strip() or "pinned commit is unavailable upstream")
                pinned_exists = git(
                    ["cat-file", "-e", f"{component.revision}^{{commit}}"],
                    cwd=repository,
                    timeout=timeout,
                )
                if pinned_exists.returncode != 0:
                    raise RuntimeError("pinned commit is unavailable in upstream history")

        if component.revision == latest:
            return "up-to-date", 0, None

        pinned_ancestor = git(
            ["merge-base", "--is-ancestor", component.revision, latest],
            cwd=repository,
            timeout=timeout,
        )
        if pinned_ancestor.returncode not in (0, 1):
            raise RuntimeError(pinned_ancestor.stderr.strip() or "unable to determine ancestry")
        if pinned_ancestor.returncode == 0:
            count = git(
                ["rev-list", "--count", f"{component.revision}..{latest}"],
                cwd=repository,
                timeout=timeout,
            )
            if count.returncode != 0:
                raise RuntimeError(count.stderr.strip() or "unable to count commits")
            return "behind", int(count.stdout.strip()), None

        latest_ancestor = git(
            ["merge-base", "--is-ancestor", latest, component.revision],
            cwd=repository,
            timeout=timeout,
        )
        if latest_ancestor.returncode == 0:
            count = git(
                ["rev-list", "--count", f"{latest}..{component.revision}"],
                cwd=repository,
                timeout=timeout,
            )
            if count.returncode != 0:
                raise RuntimeError(count.stderr.strip() or "unable to count commits")
            return "local-ahead", 0, f"local revision is {count.stdout.strip()} commit(s) ahead"

        return "diverged", None, "pinned revision is not an ancestor of the upstream ref"


def inspect(component: Component, *, exact: bool, timeout: float) -> UpdateResult:
    current = component.revision
    try:
        latest = remote_tip(component, timeout)
        if latest == current:
            return UpdateResult(component, "up-to-date", 0, current, latest)
        if not exact:
            return UpdateResult(
                component,
                "update-available",
                None,
                current,
                latest,
                "exact distance omitted; rerun with --exact",
            )
        status, behind, detail = commit_distance(component, latest, timeout)
        return UpdateResult(component, status, behind, current, latest, detail)
    except Exception as error:  # report one unavailable remote without hiding other components
        return UpdateResult(component, "error", None, current, None, str(error))


def format_result(result: UpdateResult) -> str:
    component = result.component
    current = result.current[:12]
    latest = result.latest[:12] if result.latest else "unknown"
    if result.status == "behind":
        distance = f"behind {result.behind} commit(s)"
    elif result.status == "up-to-date":
        distance = "up to date"
    elif result.status == "update-available":
        distance = "update available"
    elif result.status == "local-ahead":
        distance = "local revision is ahead"
    else:
        distance = result.status
    suffix = f" ({result.detail})" if result.detail else ""
    return f"package {component.name}: {distance}, our version is {current}, most up to date version is {latest}{suffix}"


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--component", action="append", help="check only this component; repeatable")
    parser.add_argument("--jobs", type=int, default=8, help="parallel upstream checks (default: 8)")
    parser.add_argument(
        "--exact",
        action="store_true",
        help="fetch isolated shallow history and calculate exact commit distance",
    )
    parser.add_argument(
        "--timeout",
        type=float,
        default=45.0,
        help="timeout in seconds for each Git operation (default: 45)",
    )
    parser.add_argument("--json", action="store_true", help="emit machine-readable JSON")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    if args.jobs < 1:
        raise SystemExit("--jobs must be at least 1")
    if args.timeout <= 0:
        raise SystemExit("--timeout must be greater than zero")
    components = load_components()
    requested = set(args.component or [])
    if requested:
        known = {component.name for component in components}
        unknown = sorted(requested - known)
        if unknown:
            raise SystemExit(f"unknown component(s): {', '.join(unknown)}")
        components = [component for component in components if component.name in requested]

    results: list[UpdateResult] = []
    with ThreadPoolExecutor(max_workers=min(args.jobs, len(components) or 1)) as executor:
        futures = [executor.submit(inspect, component, exact=args.exact, timeout=args.timeout) for component in components]
        for completed, future in enumerate(as_completed(futures), start=1):
            result = future.result()
            results.append(result)
            if not args.json:
                print(f"[{completed}/{len(components)}] {format_result(result)}", flush=True)
    results.sort(key=lambda result: result.component.name)

    if args.json:
        import json

        print(json.dumps([
            {
                "component": result.component.name,
                "repository": result.component.repository,
                "ref": result.component.ref,
                "source_path": result.component.source_path,
                "status": result.status,
                "behind": result.behind,
                "current": result.current,
                "latest": result.latest,
                "detail": result.detail,
            }
            for result in results
        ], indent=2, sort_keys=True))
    # Normal-mode results are printed as each worker completes. JSON remains
    # valid by suppressing progress lines and printing one final document.

    return 1 if any(result.status == "error" for result in results) else 0


if __name__ == "__main__":
    raise SystemExit(main())
