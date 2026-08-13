import os
import shutil
import subprocess
from pathlib import Path
from typing import Dict, Iterable, Sequence


class RepoError(RuntimeError):
    pass


PROJECT_TMP_RELATIVE = Path("out") / "tmp"
MIN_PROJECT_TMP_FREE_BYTES = 8 * 1024**3


def project_temp_root(repo_root: Path) -> Path:
    """Return the disk-backed temporary root owned by this MattOS checkout."""
    return repo_root / PROJECT_TMP_RELATIVE


def ensure_project_temp_root(repo_root: Path, *, minimum_free_bytes: int = MIN_PROJECT_TMP_FREE_BYTES) -> Path:
    root = project_temp_root(repo_root)
    try:
        root.mkdir(parents=True, exist_ok=True)
        usage = shutil.disk_usage(root)
    except OSError as exc:
        raise RepoError(f"cannot prepare MattOS temporary root {root}: {exc}") from exc
    if usage.free < minimum_free_bytes:
        raise RepoError(
            f"MattOS temporary root {root} has only {usage.free} free bytes; "
            f"at least {minimum_free_bytes} are required for large scratch work"
        )
    return root


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


def mattos_build_environment(repo_root: Path) -> Dict[str, str]:
    """Return the launcher environment with MattOS-owned temporary storage."""
    build_tmp = ensure_project_temp_root(repo_root)
    try:
        build_tmp.mkdir(parents=True, exist_ok=True)
        probe = build_tmp / f".launcher-write-probe-{os.getpid()}"
        probe.write_text("mattos launcher temp probe\n", encoding="utf-8")
        probe.unlink()
    except OSError as exc:
        raise RepoError(
            f"MattOS build temp directory is not writable: {build_tmp}: {exc}"
        ) from exc

    environment = os.environ.copy()
    # Always prefer repository-owned storage so a full host /tmp cannot break
    # an otherwise healthy build. This policy matches mattos-build itself.
    environment["TMPDIR"] = str(build_tmp)
    return environment


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
    env: Dict[str, str] | None = None,
) -> int:
    print("+", " ".join(args))
    if dry_run:
        return 0

    try:
        completed = subprocess.run(args, cwd=str(cwd), check=False, env=env)
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
