"""Interactive Btrfs snapshot management matching the legacy server workflow."""

from __future__ import annotations

import argparse
import os
import shutil
import subprocess
import sys
from dataclasses import dataclass
from datetime import datetime
from pathlib import Path


@dataclass(frozen=True)
class BtrfsSnapshotManager:
    """Manage snapshots below one validated Btrfs storage mount."""

    mount_point: Path
    snapshot_root: Path

    @classmethod
    def with_defaults(cls, mount_point: Path = Path("/srv/storage")) -> "BtrfsSnapshotManager":
        """Create the manager with the legacy storage and snapshot paths."""

        return cls(mount_point, mount_point / "snapshots")

    def run(self, command: tuple[str, ...], *, capture: bool = False, check: bool = True) -> subprocess.CompletedProcess[str]:
        """Run one Btrfs/system command without shell parsing."""

        return subprocess.run(command, text=True, capture_output=capture, check=check)

    def require_command(self, command: str) -> None:
        """Reject missing system dependencies before showing the manager menu."""

        if shutil.which(command) is None:
            raise RuntimeError(f"Required command '{command}' is not installed or not in PATH.")

    def filesystem_type(self) -> str:
        """Return the mounted filesystem type or an empty value when unavailable."""

        result = self.run(("findmnt", "-no", "FSTYPE", str(self.mount_point)), capture=True, check=False)
        return result.stdout.strip()

    def validate_environment(self) -> None:
        """Perform the legacy Btrfs mount, snapshot-root, and usage checks."""

        self.require_command("btrfs")
        self.require_command("findmnt")
        if not self.mount_point.is_dir():
            raise RuntimeError(f"Mount point '{self.mount_point}' does not exist.")
        filesystem_type = self.filesystem_type()
        if filesystem_type != "btrfs":
            raise RuntimeError(f"{self.mount_point} is not mounted as btrfs. Current FSTYPE: {filesystem_type or 'unknown'}")
        self.warn_snapshot_root_status()
        self.print_usage()

    def warn_snapshot_root_status(self) -> None:
        """Report whether the intended snapshot root is a Btrfs subvolume."""

        if not self.snapshot_root.is_dir():
            print(f"Warning: {self.snapshot_root} does not exist yet.")
            print("If you want snapshots there, create it (preferably as a Btrfs subvolume).")
            return
        result = self.run(("btrfs", "subvolume", "show", str(self.snapshot_root)), check=False)
        if result.returncode == 0:
            print(f"Snapshot root check: {self.snapshot_root} is a Btrfs subvolume.")
        else:
            print(f"Warning: {self.snapshot_root} exists but is NOT a Btrfs subvolume.")
            print("Snapshots can still be placed there, but intended setup is a subvolume.")

    def print_usage(self) -> None:
        """Display Btrfs usage without making inability to inspect fatal."""

        print(f"=== Btrfs Filesystem Usage ({self.mount_point}) ===")
        self.run(("btrfs", "filesystem", "usage", str(self.mount_point)), check=False)
        print()

    @staticmethod
    def parse_subvolumes(output: str) -> list[tuple[str, str, str]]:
        """Extract ID, top-level ID, and path from btrfs subvolume list output."""

        entries: list[tuple[str, str, str]] = []
        for line in output.splitlines():
            tokens = line.split()
            try:
                identifier = tokens[tokens.index("ID") + 1]
                top_index = tokens.index("top")
                top_level = tokens[top_index + 2] if tokens[top_index + 1] == "level" else tokens[top_index + 1]
                path_index = tokens.index("path") + 1
            except (ValueError, IndexError):
                continue
            entries.append((identifier, top_level, " ".join(tokens[path_index:])))
        return entries

    def list_subvolumes(self) -> None:
        """Print Btrfs subvolume, mount, and filesystem details."""

        result = self.run(("btrfs", "subvolume", "list", "-p", str(self.mount_point)), capture=True)
        print("=== Subvolumes (ID | TOP_LEVEL | PATH) ===")
        for identifier, top_level, path in self.parse_subvolumes(result.stdout):
            print(f"{identifier:<8} {top_level:<10} {path}")
        print()
        print(f"=== btrfs subvolume show {self.mount_point} ===")
        self.run(("btrfs", "subvolume", "show", str(self.mount_point)), check=False)
        print()
        print(f"=== findmnt -no SOURCE,OPTIONS {self.mount_point} ===")
        self.run(("findmnt", "-no", "SOURCE,OPTIONS", str(self.mount_point)), check=False)
        print()

    def prompt(self, question: str) -> str:
        """Read one terminal answer, converting end-of-input into cancellation."""

        try:
            return input(question).strip()
        except EOFError:
            return ""

    def create_snapshot(self) -> None:
        """Create a read-only-by-default snapshot inside the protected root."""

        source = Path(self.prompt(f"Enter source subvolume path [{self.mount_point}]: ") or self.mount_point)
        default_destination = self.snapshot_root / f"@data-{datetime.now():%Y-%m-%d-%H%M}"
        destination = Path(self.prompt(f"Enter destination snapshot path [{default_destination}]: ") or default_destination)
        if not source.is_dir():
            print(f"Error: source path does not exist: {source}")
            return
        if self.run(("btrfs", "subvolume", "show", str(source)), check=False).returncode != 0:
            print(f"Error: source is not a Btrfs subvolume: {source}")
            return
        if destination.exists():
            print(f"Error: destination already exists: {destination}")
            return
        try:
            destination.relative_to(self.snapshot_root)
        except ValueError:
            print(f"Error: destination must be inside {self.snapshot_root}.")
            return
        read_only = self.prompt("Create read-only snapshot? [Y/n]: ").lower() not in {"n", "no"}
        destination.parent.mkdir(parents=True, exist_ok=True)
        command = ("btrfs", "subvolume", "snapshot", "-r", str(source), str(destination)) if read_only else (
            "btrfs", "subvolume", "snapshot", str(source), str(destination)
        )
        self.run(command)
        mode = "read-only" if read_only else "read-write"
        print(f"Created {mode} snapshot: {destination}")

    def snapshot_entries(self) -> list[str]:
        """Return only snapshot subvolume paths rooted below snapshots/."""

        result = self.run(("btrfs", "subvolume", "list", "-p", str(self.mount_point)), capture=True, check=False)
        return [path for _, _, path in self.parse_subvolumes(result.stdout) if path.startswith("snapshots/")]

    def delete_snapshot(self) -> None:
        """Delete one listed snapshot only after exact-name and final confirmation."""

        entries = self.snapshot_entries()
        if not entries:
            print("No snapshot subvolumes found under snapshots/.")
            return
        print(f"=== Snapshots under {self.snapshot_root} ===")
        for index, path in enumerate(entries, start=1):
            print(f"{index:3}) {path}")
        selected = self.prompt("Select snapshot number to delete: ")
        if not selected.isdigit() or not 1 <= int(selected) <= len(entries):
            print("Error: invalid selection." if not selected.isdigit() else "Error: selection out of range.")
            return
        selected_relative = entries[int(selected) - 1]
        selected_path = self.mount_point / selected_relative
        try:
            selected_path.relative_to(self.snapshot_root)
        except ValueError:
            print(f"Safety stop: refusing to delete outside {self.snapshot_root}.")
            return
        if not selected_path.exists():
            print(f"Error: snapshot path no longer exists: {selected_path}")
            return
        selected_name = selected_relative.removeprefix("snapshots/")
        print(f"Selected snapshot: {selected_relative}")
        if self.prompt(f"Type exact snapshot name to confirm deletion ('{selected_name}'): ") != selected_name:
            print("Confirmation name mismatch. Deletion canceled.")
            return
        if self.prompt(f"Final confirmation: delete '{selected_relative}'? (y/N): ").lower() != "y":
            print("Deletion canceled.")
            return
        self.run(("btrfs", "subvolume", "delete", str(selected_path)))
        print(f"Deleted snapshot: {selected_relative}")

    def menu(self) -> int:
        """Run the legacy-compatible interactive snapshot manager menu."""

        self.validate_environment()
        while True:
            print("=== Btrfs Snapshot Manager ===")
            print(f"Mount point: {self.mount_point}")
            print(f"Snapshot root: {self.snapshot_root}")
            print("\n1) List subvolumes\n2) Create snapshot\n3) Delete snapshot\n4) Show btrfs usage\n5) Exit\n")
            choice = self.prompt("Choose an option [1-5]: ")
            if choice == "1":
                self.list_subvolumes()
            elif choice == "2":
                self.create_snapshot()
            elif choice == "3":
                self.delete_snapshot()
            elif choice == "4":
                self.print_usage()
            elif choice == "5" or not choice:
                print("Goodbye.")
                return 0
            else:
                print("Invalid selection.")
            print()


def main(argv: list[str] | None = None) -> int:
    """Run the snapshot manager directly for compatibility and testing."""

    if os.name != "nt" and os.geteuid() != 0:
        print("Re-running with sudo...")
        os.execvp("sudo", ("sudo", sys.executable, str(Path(__file__).resolve()), *(argv or sys.argv[1:])))
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--mount-point", type=Path, default=Path("/srv/storage"), help="Btrfs storage mount point.")
    args = parser.parse_args(argv)
    return BtrfsSnapshotManager.with_defaults(args.mount_point).menu()


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, RuntimeError, subprocess.CalledProcessError) as error:
        print(f"Error: {error}", file=sys.stderr)
        raise SystemExit(1) from error