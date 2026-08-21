#!/usr/bin/env python3
"""Interactively queue and run LinuxScripts container workloads."""

from __future__ import annotations

import os
import shutil
import subprocess
import sys
from pathlib import Path


REPOSITORY_ROOT = Path(__file__).resolve().parents[1]
SOURCE_DIRECTORY = REPOSITORY_ROOT / "src"


def use_project_interpreter() -> None:
    """Use the bootstrapped virtual environment when one is available."""

    venv_python = REPOSITORY_ROOT / (".venv/Scripts/python.exe" if os.name == "nt" else ".venv/bin/python")
    if venv_python.is_file() and Path(sys.executable).resolve() != venv_python.resolve():
        os.execv(str(venv_python), (str(venv_python), *sys.argv))


use_project_interpreter()
if str(SOURCE_DIRECTORY) not in sys.path:
    sys.path.insert(0, str(SOURCE_DIRECTORY))

from containers.workloads import Action, WORKLOADS, run_workload


DISPLAY_NAMES = {
    "homepage": "src/containers/run_homepage.py",
    "jellyfin": "src/containers/run_jellyfin.py",
    "ollama": "src/containers/run_ollama.py",
    "portainer": "src/containers/run_portainer.py",
    "stable-diffusion": "src/containers/run_stable_diffusion.py",
}


def docker_prefix() -> tuple[str, ...] | None:
    """Return an accessible Docker command without attempting installation."""

    if shutil.which("docker") is None:
        return None
    if subprocess.run(("docker", "info"), check=False, capture_output=True).returncode == 0:
        return ("docker",)
    if subprocess.run(("sudo", "docker", "info"), check=False, capture_output=True).returncode == 0:
        return ("sudo", "docker")
    return None


def print_container_table() -> None:
    """Show existing containers before queueing legacy-compatible actions."""

    print("=== System Containers ===")
    prefix = docker_prefix()
    if prefix is None:
        print("Docker is not installed or its daemon is unavailable; cannot list containers.\n")
        return
    result = subprocess.run((*prefix, "ps", "-a", "--format", "{{.Names}}|{{.Status}}"), check=False, capture_output=True, text=True)
    rows = [line.partition("|") for line in result.stdout.splitlines() if "|" in line]
    if not rows:
        print("No containers found.\n")
        return
    print(f"{'NAME':<40} STATUS")
    for name, _, status in rows:
        print(f"{name:<40} {status}")
    print()


def choose_actions() -> list[tuple[str, Action]]:
    """Preserve ContainerManager.sh's queue-then-run prompt semantics."""

    queued: list[tuple[str, Action]] = []
    print("=== Container Scripts ===")
    print("Use -I for no-flag install/default behavior. No workload runs until every prompt is answered.")
    stop_prompting = False
    for name in WORKLOADS:
        display_name = DISPLAY_NAMES[name]
        if stop_prompting:
            print(f"Skipping {display_name} (--end requested).")
            continue
        while True:
            try:
                entered = input(f"{display_name}: enter one of [--on/--off/--delete/-I/--skip/--end]: ").strip()
            except EOFError:
                return queued
            if entered == "--end":
                print("Stopping prompts. Remaining workloads will be skipped.")
                stop_prompting = True
                break
            if entered == "--skip":
                print(f"Skipping {display_name}.")
                break
            action = {"--on": Action.ON, "--off": Action.OFF, "--delete": Action.DELETE, "-I": Action.RUN}.get(entered)
            if action is not None:
                queued.append((name, action))
                print(f"Queued {display_name} {entered}")
                break
            print("Invalid input. Use: --on, --off, --delete, -I, --skip, or --end.")
    return queued


def main() -> int:
    """Run the backwards-compatible interactive container manager."""

    if len(sys.argv) != 1:
        print("ContainerManager.py is interactive and does not accept command-line arguments.", file=sys.stderr)
        return 1
    print_container_table()
    queued = choose_actions()
    print("\n=== Execution Phase ===")
    if not queued:
        print("No actions queued. Nothing to run.")
        return 0
    for name, action in queued:
        print(f"Running {DISPLAY_NAMES[name]} ({action.value})")
        try:
            run_workload(name, action)
        except (OSError, RuntimeError, subprocess.CalledProcessError) as error:
            print(f"{DISPLAY_NAMES[name]} failed: {error}", file=sys.stderr)
    print("Container setup complete.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())