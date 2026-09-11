#!/usr/bin/env python3
"""Discover and update every independent MattOS third-party package recipe.

This is deliberately an orchestration layer: recipes remain the authority for
their upstream/version/build policy, and the common framework remains the
authority for repository selection and publication.  Every recipe is invoked
in a separate process so a broken upstream cannot prevent the remaining
packages from being checked.
"""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
import tempfile
from dataclasses import dataclass
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
RECIPES = ROOT / "third-party-packages"
SKIP = frozenset({"__init__.py"})


@dataclass(frozen=True)
class Outcome:
    recipe: str
    status: str
    detail: str
    output: str


class Colors:
    def __init__(self, enabled: bool):
        self.enabled = enabled

    def paint(self, code: str, text: str) -> str:
        return f"\033[{code}m{text}\033[0m" if self.enabled else text

    def status(self, status: str) -> str:
        palette = {
            "UP TO DATE": "36", "UPLOADED": "32", "DRY RUN": "33", "FAILED": "31",
        }
        return self.paint(f"1;{palette[status]}", status)


def discover_recipes(root: Path = RECIPES) -> list[Path]:
    """Only immediate, executable recipe modules are eligible for bulk work."""
    return sorted(
        path for path in root.glob("*.py")
        if path.name not in SKIP and not path.name.startswith("_")
    )


def tail(text: str, lines: int = 8) -> str:
    values = [line for line in text.rstrip().splitlines() if line.strip()]
    return "\n".join(values[-lines:]) if values else "recipe exited without diagnostic output"


def failure_summary(text: str) -> str:
    """Keep the normal table legible while retaining full captured output."""
    values = [line.strip() for line in text.splitlines() if line.strip()]
    for line in reversed(values):
        if line.startswith("error:"):
            return line.removeprefix("error:").strip()
    return tail(text, lines=1)


def invoke(recipe: Path, *, dry_run: bool) -> Outcome:
    with tempfile.TemporaryDirectory(prefix="mattos-third-party-result-", dir=ROOT / "out/tmp") as temp:
        result = Path(temp) / "result.json"
        args = [sys.executable, str(recipe), "update", "--result-json", str(result)]
        if dry_run:
            args.append("--dry-run")
        completed = subprocess.run(args, cwd=ROOT, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        output = completed.stdout
        if completed.returncode:
            return Outcome(recipe.stem, "FAILED", f"exit {completed.returncode}: {failure_summary(output)}", output)
        try:
            data = json.loads(result.read_text(encoding="utf-8"))
            status = str(data["status"])
            version = str(data["version"])
        except (OSError, KeyError, TypeError, json.JSONDecodeError) as exc:
            return Outcome(recipe.stem, "FAILED", f"recipe returned no valid result manifest: {exc}", output)
        labels = {
            "up-to-date": "UP TO DATE",
            "uploaded": "UPLOADED",
            "dry-run": "DRY RUN",
        }
        label = labels.get(status)
        if not label:
            return Outcome(recipe.stem, "FAILED", f"recipe returned unknown status {status!r}", output)
        return Outcome(recipe.stem, label, version, output)


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description="Check, build, and publish all MattOS third-party recipes")
    parser.add_argument("--dry-run", action="store_true", help="pass --dry-run to recipe publication")
    parser.add_argument("--recipe", action="append", default=[], metavar="NAME", help="run only a discovered recipe (repeatable)")
    parser.add_argument("--color", choices=("auto", "always", "never"), default="auto")
    parser.add_argument("--verbose", action="store_true", help="print each recipe's complete captured output")
    args = parser.parse_args(argv)
    (ROOT / "out/tmp").mkdir(parents=True, exist_ok=True)
    enabled = args.color == "always" or (args.color == "auto" and sys.stdout.isatty())
    colors = Colors(enabled)
    recipes = discover_recipes()
    requested = set(args.recipe)
    if requested:
        recipes = [recipe for recipe in recipes if recipe.stem in requested]
        missing = requested - {recipe.stem for recipe in recipes}
        if missing:
            parser.error("unknown third-party recipe(s): " + ", ".join(sorted(missing)))
    if not recipes:
        parser.error("no third-party package recipes were discovered")
    mode = "DRY RUN" if args.dry_run else "UPDATE AND PUBLISH"
    print(colors.paint("1", f"MattOS third-party packages — {mode}"))
    print(f"Discovered {len(recipes)} recipe(s): " + ", ".join(recipe.stem for recipe in recipes))
    outcomes: list[Outcome] = []
    for recipe in recipes:
        print(f"\n{colors.paint('1', f'[{recipe.stem}]')} checking upstream and its declared repository…", flush=True)
        outcome = invoke(recipe, dry_run=args.dry_run)
        outcomes.append(outcome)
        print(f"{colors.status(outcome.status):<20} {recipe.stem:<16} {outcome.detail}", flush=True)
        if args.verbose and outcome.output.strip():
            print(outcome.output.rstrip())
    print("\n" + colors.paint("1", "Summary"))
    for outcome in outcomes:
        print(f"{colors.status(outcome.status):<20} {outcome.recipe:<16} {outcome.detail}")
    failed = [outcome for outcome in outcomes if outcome.status == "FAILED"]
    print(f"\n{len(outcomes) - len(failed)}/{len(outcomes)} recipe(s) completed successfully.")
    return 1 if failed else 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
