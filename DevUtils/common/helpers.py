import os
import re
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


def ensure_source_ownership_overrides(repo_root: Path) -> None:
    """Generate the Cargo source-ownership catalog before Cargo starts."""
    generator = repo_root / "DevUtils" / "generate_source_overrides.py"
    if not generator.is_file():
        return
    try:
        completed = subprocess.run(
            ["python3", str(generator)],
            cwd=str(repo_root),
            check=False,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
        )
    except FileNotFoundError as exc:
        raise RepoError("python3 is required to generate MattOS source ownership metadata") from exc
    if completed.returncode != 0:
        detail = completed.stderr.strip() or completed.stdout.strip()
        suffix = f": {detail}" if detail else ""
        raise RepoError(f"failed to generate MattOS source ownership metadata{suffix}")


def prepare_cargo_dispatcher(repo_root: Path) -> tuple[Path, Path]:
    """Return (dispatcher_dir, real cargo proxy) without shadowing unrelated workspaces.

    Do not resolve the cargo path through symlinks. rustup deliberately installs
    ~/.cargo/bin/cargo as a proxy symlink to the rustup binary and selects Cargo
    from argv[0]. Dereferencing that symlink would execute `rustup` as rustup and
    make normal Cargo flags such as `-p` get parsed as rustup arguments.
    """
    real_cargo = shutil.which("cargo")
    if not real_cargo:
        raise RepoError("cargo is required to prepare MattOS source ownership")
    real_cargo_path = Path(real_cargo).absolute()
    if real_cargo_path.name != "cargo":
        raise RepoError(f"resolved Cargo command does not preserve cargo proxy identity: {real_cargo_path}")
    dispatcher_source = repo_root / "DevUtils" / "cargo_source_owned.py"
    if not dispatcher_source.is_file():
        raise RepoError(f"missing MattOS Cargo dispatcher: {dispatcher_source}")
    dispatcher_dir = repo_root / "out" / "source-ownership" / "bin"
    dispatcher_dir.mkdir(parents=True, exist_ok=True)
    dispatcher = dispatcher_dir / "cargo"
    shutil.copy2(dispatcher_source, dispatcher)
    dispatcher.chmod(0o755)
    return dispatcher_dir, real_cargo_path


def mattos_build_environment(repo_root: Path) -> Dict[str, str]:
    """Return the launcher environment with source ownership and disk-backed temp."""
    ensure_source_ownership_overrides(repo_root)
    dispatcher_dir, real_cargo = prepare_cargo_dispatcher(repo_root)

    build_tmp = ensure_project_temp_root(repo_root)
    try:
        probe = build_tmp / f".launcher-write-probe-{os.getpid()}"
        probe.write_text("mattos launcher temp probe\n", encoding="utf-8")
        probe.unlink()
    except OSError as exc:
        raise RepoError(f"MattOS build temp directory is not writable: {build_tmp}: {exc}") from exc

    environment = os.environ.copy()
    environment["TMPDIR"] = str(build_tmp)
    environment["MATTOS_REPO_ROOT"] = str(repo_root.resolve())
    environment["MATTOS_REAL_CARGO"] = str(real_cargo)
    environment["PATH"] = str(dispatcher_dir) + os.pathsep + environment.get("PATH", "")
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
        if len(value) >= 2 and value[0] == value[-1] and value[0] in {'\"', "'"}:
            value = value[1:-1]
        data[key] = value

    if "ID" not in data:
        raise RepoError(f"invalid {path}: missing ID field")

    return data


def _ownership_log_failed(text: str) -> bool:
    if "prepare_error=" in text or "ownership_error=" in text:
        return True
    if re.search(r"(?m)^(?:metadata|final)_status=(?!0$)\d+$", text):
        return True
    match = re.search(r"(?m)^ownership_failures=(.+)$", text)
    return bool(match and match.group(1).strip() not in {"[]", "null", ""})


def _dump_source_ownership_failure_logs(cwd: Path) -> None:
    """Surface dispatcher diagnostics when a launcher command fails.

    mattos-build may aggregate parallel stage output, so the child Cargo stderr
    is not guaranteed to remain visible in the outer command's terminal stream.
    The dispatcher already persists exact diagnostics under out/. Print failed
    ownership logs here so a normal run_qemu.py transcript is self-contained.
    """
    try:
        repo_root = find_repo_root(cwd)
    except RepoError:
        return
    logs_dir = repo_root / "out" / "source-ownership" / "logs"
    if not logs_dir.is_dir():
        return

    failed: list[tuple[float, Path, str]] = []
    for path in logs_dir.glob("*.log"):
        try:
            text = path.read_text(encoding="utf-8", errors="replace")
            if _ownership_log_failed(text):
                failed.append((path.stat().st_mtime, path, text))
        except OSError:
            continue

    for _, path, text in sorted(failed, reverse=True)[:4]:
        print(f"\n===== MattOS source ownership failure: {path.relative_to(repo_root)} =====", file=os.sys.stderr)
        print(text.rstrip(), file=os.sys.stderr)
        print("===== end source ownership failure =====\n", file=os.sys.stderr)


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
        _dump_source_ownership_failure_logs(cwd)
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
