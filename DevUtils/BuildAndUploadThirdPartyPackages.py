#!/usr/bin/env python3
"""Check or publish independent MattOS third-party package recipes.

The repository inventory is fetched once per declared repository, then passed
to every isolated recipe as a checked snapshot. Consequently status reporting
never builds packages, and checking ten recipes does not perform ten remote
``list`` operations.
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
import tempfile
from dataclasses import dataclass
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
RECIPES = ROOT / "third-party-packages"
SKIP = frozenset({"__init__.py"})
sys.path.insert(0, str(RECIPES))
from common import RecipeError, repository_inventory, write_repository_inventory  # noqa: E402


@dataclass(frozen=True)
class RecipeDescriptor:
    path: Path
    package: str
    repository: str
    selected_version: str


@dataclass(frozen=True)
class Outcome:
    recipe: str
    status: str
    upstream: str
    selected: str
    published: str
    detail: str
    output: str


class Colors:
    def __init__(self, enabled: bool):
        self.enabled = enabled

    def paint(self, code: str, text: str) -> str:
        return f"\033[{code}m{text}\033[0m" if self.enabled else text

    def status(self, status: str) -> str:
        palette = {
            "UP TO DATE": "32", "UPSTREAM NEWER": "33", "PENDING PUBLISH": "33",
            "REPOSITORY DIVERGED": "33", "UPLOADED": "32", "DRY RUN": "33", "FAILED": "31",
        }
        return self.paint(f"1;{palette[status]}", status)


def discover_recipes(root: Path = RECIPES) -> list[Path]:
    return sorted(path for path in root.glob("*.py") if path.name not in SKIP and not path.name.startswith("_"))


def tail(text: str, lines: int = 8) -> str:
    values = [line for line in text.rstrip().splitlines() if line.strip()]
    return "\n".join(values[-lines:]) if values else "recipe exited without diagnostic output"


def failure_summary(text: str) -> str:
    values = [line.strip() for line in text.splitlines() if line.strip()]
    for line in reversed(values):
        if line.startswith("error:"):
            return line.removeprefix("error:").strip()
    return tail(text, lines=1)


def descriptor(path: Path) -> RecipeDescriptor:
    completed = subprocess.run(
        [sys.executable, str(path), "--describe-json"], cwd=ROOT, text=True,
        stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
    )
    if completed.returncode:
        raise RecipeError(f"{path.stem}: {failure_summary(completed.stdout)}")
    try:
        data = json.loads(completed.stdout)
        return RecipeDescriptor(path, str(data["package"]), str(data["repository"]), str(data["selected_version"]))
    except (KeyError, TypeError, json.JSONDecodeError) as exc:
        raise RecipeError(f"{path.stem}: invalid recipe descriptor: {exc}") from exc


def invoke(recipe: RecipeDescriptor, *, mode: str, dry_run: bool, inventory: Path) -> Outcome:
    with tempfile.TemporaryDirectory(prefix="mattos-third-party-result-", dir=ROOT / "out/tmp") as temp:
        result = Path(temp) / "result.json"
        arguments = [sys.executable, str(recipe.path), mode, "--result-json", str(result),
                     "--repository-inventory", str(inventory)]
        if dry_run:
            arguments.append("--dry-run")
        completed = subprocess.run(arguments, cwd=ROOT, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        output = completed.stdout
        if completed.returncode:
            return Outcome(recipe.package, "FAILED", "?", recipe.selected_version, "?",
                           f"exit {completed.returncode}: {failure_summary(output)}", output)
        try:
            data = json.loads(result.read_text(encoding="utf-8"))
            status = str(data.get("status", ""))
            state = str(data.get("release_state")) if status == "checked" else status
            upstream = str(data["upstream_version"])
            selected = str(data["selected_version"])
            published = str(data.get("repository_version") or "not published")
        except (OSError, KeyError, TypeError, json.JSONDecodeError) as exc:
            return Outcome(recipe.package, "FAILED", "?", recipe.selected_version, "?",
                           f"recipe returned no valid result manifest: {exc}", output)
        labels = {
            "up-to-date": "UP TO DATE", "upstream-newer": "UPSTREAM NEWER",
            "pending-publish": "PENDING PUBLISH", "repository-diverged": "REPOSITORY DIVERGED",
            "uploaded": "UPLOADED", "dry-run": "DRY RUN",
        }
        label = labels.get(state)
        if not label:
            return Outcome(recipe.package, "FAILED", upstream, selected, published,
                           f"recipe returned unknown status {state!r}", output)
        detail = "selected release is already published" if state == "up-to-date" else state.replace("-", " ")
        return Outcome(recipe.package, label, upstream, selected, published, detail, output)


def print_outcome(colors: Colors, outcome: Outcome) -> None:
    print(
        f"{colors.status(outcome.status):<25} {outcome.recipe:<14} "
        f"upstream {outcome.upstream:<12} selected {outcome.selected:<12} "
        f"repo {outcome.published:<14} {outcome.detail}", flush=True,
    )


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description="Check or update all MattOS third-party package recipes")
    parser.add_argument("--check", action="store_true", help="read-only three-version status report (never builds)")
    parser.add_argument("--dry-run", action="store_true", help="validate publication without uploading")
    parser.add_argument("--recipe", action="append", default=[], metavar="NAME", help="run only a discovered recipe (repeatable)")
    parser.add_argument("--color", choices=("auto", "always", "never"), default="auto")
    parser.add_argument("--verbose", action="store_true", help="print complete captured output for each recipe")
    args = parser.parse_args(argv)
    (ROOT / "out/tmp").mkdir(parents=True, exist_ok=True)
    colors = Colors(args.color == "always" or (args.color == "auto" and sys.stdout.isatty()))
    paths = discover_recipes()
    requested = set(args.recipe)
    if requested:
        paths = [path for path in paths if path.stem in requested]
        missing = requested - {path.stem for path in paths}
        if missing:
            parser.error("unknown third-party recipe(s): " + ", ".join(sorted(missing)))
    if not paths:
        parser.error("no third-party package recipes were discovered")
    try:
        recipes = [descriptor(path) for path in paths]
    except RecipeError as exc:
        parser.error(str(exc))
    mode = "check" if args.check else "update"
    title = "READ-ONLY STATUS" if args.check else ("UPDATE AND PUBLISH" if not args.dry_run else "DRY-RUN UPDATE")
    print(colors.paint("1", f"MattOS third-party packages — {title}"))
    print("Columns: upstream latest | MattOS selected release | newest published repository version")
    print("Inventory: one checked query per declared repository; no package is built to determine status.")
    grouped: dict[str, list[RecipeDescriptor]] = {}
    for recipe in recipes:
        grouped.setdefault(recipe.repository, []).append(recipe)
    outcomes: list[Outcome] = []
    with tempfile.TemporaryDirectory(prefix="mattos-third-party-inventory-", dir=ROOT / "out/tmp") as temporary:
        snapshots: dict[str, Path] = {}
        for repository in grouped:
            try:
                inventory = repository_inventory(ROOT, repository)
                snapshot = Path(temporary) / f"{repository}.json"
                write_repository_inventory(snapshot, repository, inventory)
                snapshots[repository] = snapshot
                print(f"Repository {repository}: inventory loaded once ({len(inventory)} package name(s)).")
            except RecipeError as exc:
                print(colors.status("FAILED") + f" repository {repository}: {exc}")
                for recipe in grouped[repository]:
                    outcomes.append(Outcome(recipe.package, "FAILED", "?", recipe.selected_version, "?", str(exc), ""))
        for recipe in recipes:
            snapshot = snapshots.get(recipe.repository)
            if snapshot:
                outcome = invoke(recipe, mode=mode, dry_run=args.dry_run, inventory=snapshot)
                outcomes.append(outcome)
                print_outcome(colors, outcome)
                if args.verbose and outcome.output.strip():
                    print(outcome.output.rstrip())
    print("\n" + colors.paint("1", "Summary"))
    for outcome in outcomes:
        print_outcome(colors, outcome)
    failed = [outcome for outcome in outcomes if outcome.status == "FAILED"]
    print(f"\n{len(outcomes) - len(failed)}/{len(outcomes)} recipe(s) completed successfully.")
    return 1 if failed else 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
