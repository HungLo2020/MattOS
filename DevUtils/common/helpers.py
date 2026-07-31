import shutil
import subprocess
from pathlib import Path
from typing import Dict, Iterable, Sequence


class RepoError(RuntimeError):
    pass


def find_repo_root(start: Path) -> Path:
    current = start.resolve()
    if current.is_file():
        current = current.parent

    for candidate in [current, *current.parents]:
        if _looks_like_repo_root(candidate):
            return candidate

    raise RepoError(f"unable to find MattOS repository root from {start}")


def _looks_like_repo_root(path: Path) -> bool:
    return (
        (path / "Cargo.toml").is_file()
        and (path / "src" / "tools" / "mattos-build" / "Cargo.toml").is_file()
        and (path / "upstream" / "sources.toml").is_file()
    )


def ensure_tools(tools: Iterable[str]) -> None:
    missing = [tool for tool in tools if shutil.which(tool) is None]
    if missing:
        raise RepoError(f"missing required tools: {', '.join(missing)}")


def command_exists(tool: str) -> bool:
    return shutil.which(tool) is not None


def read_os_release(path: Path = Path("/etc/os-release")) -> Dict[str, str]:
    try:
        raw = path.read_text(encoding="utf-8")
    except OSError as exc:
        raise RepoError(f"failed to read {path}: {exc}") from exc

    data: Dict[str, str] = {}
    for line in raw.splitlines():
        line = line.strip()
        if not line or line.startswith("#") or "=" not in line:
            continue
        key, value = line.split("=", 1)
        value = value.strip()
        if len(value) >= 2 and value[0] == value[-1] and value[0] in {'"', "'"}:
            value = value[1:-1]
        data[key] = value

    if "ID" not in data:
        raise RepoError(f"invalid {path}: missing ID field")

    return data


def run_command(
    args: Sequence[str],
    cwd: Path,
    dry_run: bool = False,
    check: bool = True,
) -> int:
    print("+", " ".join(args))
    if dry_run:
        return 0

    try:
        completed = subprocess.run(args, cwd=str(cwd), check=False)
    except FileNotFoundError as exc:
        raise RepoError(f"failed to execute {args[0]}: {exc}") from exc

    if check and completed.returncode != 0:
        raise RepoError(
            f"command failed with exit code {completed.returncode}: {' '.join(args)}"
        )

    return completed.returncode


def run_command_capture(args: Sequence[str], cwd: Path) -> str:
    try:
        completed = subprocess.run(
            args,
            cwd=str(cwd),
            check=True,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )
    except FileNotFoundError as exc:
        raise RepoError(f"failed to execute {args[0]}: {exc}") from exc
    except subprocess.CalledProcessError as exc:
        stderr = exc.stderr.strip() if exc.stderr else ""
        detail = f"; stderr: {stderr}" if stderr else ""
        raise RepoError(
            f"command failed with exit code {exc.returncode}: {' '.join(args)}{detail}"
        ) from exc

    return completed.stdout
