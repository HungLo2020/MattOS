#!/usr/bin/env python3
"""Small host-side QMP client for deterministic MattOS graphical tests.

This is deliberately development infrastructure, not guest software. Start a
graphical ISO with ``DevUtils/run_qemu.py --test-control`` and use this helper
while that QEMU process remains alive:

  python3 DevUtils/qemu_test_control.py screenshot out/qemu/test-control/cosmic.ppm
  python3 DevUtils/qemu_test_control.py key ctrl-alt-t
  python3 DevUtils/qemu_test_control.py text 'flatpak --user list'
  python3 DevUtils/qemu_test_control.py click 640 400
  python3 DevUtils/qemu_test_control.py serial 'pgrep -af cosmic-comp'

The QMP protocol is local Unix-socket only. Commands time out by default and
never start or retain a guest-side automation service.
"""

from __future__ import annotations

import argparse
import json
import re
import socket
import sys
import tempfile
import time
from pathlib import Path
from collections.abc import Callable
from typing import Any


DEFAULT_SOCKET = Path("out/qemu/test-control/qmp.sock")
DEFAULT_SERIAL_SOCKET = Path("out/qemu/test-control/serial.sock")
QMP_TIMEOUT_SECONDS = 10.0
SERIAL_PROMPT_WAKEUP = b"\r"
GRUB_PROMPT = b"grub>"
LIVE_USERSPACE_SIGNATURES = (b"systemd[", b"MattOS Live Environment", b"login:")
# Brush emits OSC shell-integration records immediately before its prompt, so
# a live prompt is not necessarily preceded by a terminal newline.  The fresh
# marker probe below is the actual proof that this is a shell; this expression
# only decides when it is safe to send that harmless probe.
# The serial UART can interleave a kernel line immediately after a prompt
# (for example ``mattos@host:~$ [  8.4] clocksource...``).  Anchor on the
# normal user@host:path shell shape and accept that interleaving, while not
# treating arbitrary boot-loader text or echoed commands as a shell prompt.
SHELL_PROMPT = re.compile(
    rb"(?:^|[\r\n])(?:\x1b\][^\r\n]*?(?:\x1b\\|\x07))*"
    rb"[A-Za-z0-9_.-]+@[A-Za-z0-9_.-]+:[^\r\n]{0,160}[$#] ?"
    rb"(?=\r?\n|\r|$|\[)"
)


class QmpError(RuntimeError):
    pass


class QmpClient:
    def __init__(self, socket_path: Path, timeout: float) -> None:
        self.socket_path = socket_path
        self.timeout = timeout
        self.socket: socket.socket | None = None
        self._reader: Any = None
        self._next_id = 1

    def __enter__(self) -> "QmpClient":
        connection = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        connection.settimeout(self.timeout)
        connection.connect(str(self.socket_path))
        self.socket = connection
        self._reader = connection.makefile("rb")
        greeting = self._receive()
        if "QMP" not in greeting:
            raise QmpError(f"unexpected QMP greeting: {greeting}")
        self.execute("qmp_capabilities")
        return self

    def __exit__(self, *_: object) -> None:
        if self._reader is not None:
            self._reader.close()
        if self.socket is not None:
            self.socket.close()

    def _receive(self) -> dict[str, Any]:
        if self._reader is None:
            raise QmpError("QMP connection is not open")
        raw = self._reader.readline()
        if not raw:
            raise QmpError("QMP connection closed")
        try:
            return json.loads(raw)
        except json.JSONDecodeError as exc:
            raise QmpError(f"invalid QMP response: {raw!r}") from exc

    def execute(self, command: str, arguments: dict[str, Any] | None = None) -> dict[str, Any]:
        if self.socket is None:
            raise QmpError("QMP connection is not open")
        request_id = self._next_id
        self._next_id += 1
        request: dict[str, Any] = {"execute": command, "id": request_id}
        if arguments:
            request["arguments"] = arguments
        self.socket.sendall(json.dumps(request, separators=(",", ":")).encode("utf-8") + b"\r\n")
        while True:
            response = self._receive()
            if response.get("id") != request_id:
                continue
            if "error" in response:
                detail = response["error"].get("desc", response["error"])
                raise QmpError(f"QMP {command} failed: {detail}")
            return response.get("return", {})


def wait_for_socket(socket_path: Path, timeout: float) -> None:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if socket_path.is_socket():
            try:
                with QmpClient(socket_path, min(1.0, timeout)):
                    return
            except (OSError, QmpError):
                pass
        time.sleep(0.1)
    raise QmpError(f"QMP socket did not become ready within {timeout:g}s: {socket_path}")


def serial_console_is_grub(output: bytes) -> bool:
    """Return whether the serial stream is owned by the GRUB command shell."""
    return GRUB_PROMPT in output.lower()


def serial_console_has_shell_prompt(output: bytes) -> bool:
    """Recognize a POSIX shell prompt, never a boot-loader prompt."""
    return SHELL_PROMPT.search(output) is not None


def shell_probe_payload(marker: str) -> bytes:
    """Return a harmless, unique command used to prove a live shell owns serial."""
    return f"printf '\\n{marker}\\n'\r".encode("utf-8")


def shell_probe_satisfied(output: bytes, marker: str) -> bool:
    """Accept only the probe command's output frame, never its terminal echo.

    A serial console echoes the literal ``printf '\\nMARKER\\n'`` before the
    shell has executed it.  Treating that echo as readiness lets the next
    command be injected while the shell still owns the probe input.  Require
    real serial line boundaries, which the printf output has and the escaped
    command echo does not.
    """
    marker_bytes = marker.encode("utf-8")
    return re.search(
        rb"(?:\r\n|\n|\r)" + re.escape(marker_bytes) + rb"(?:\r\n|\n|\r)",
        output,
    ) is not None


def wait_for_live_shell(
    connection: socket.socket,
    timeout: float,
    *,
    on_output: Callable[[str], None] | None = None,
) -> None:
    """Wait for, then positively probe, the MattOS userspace serial shell.

    QMP being reachable says only that QEMU has started.  In particular, OVMF
    and GRUB can own the same serial device before Linux starts.  This function
    normally waits for userspace output before sending input.  A serial socket
    can also be attached after boot, however, in which case QEMU has no boot
    transcript to replay.  After a short *silent-attach* interval this function
    sends exactly one harmless carriage return to redraw the current console.
    It immediately rejects a resulting ``grub>`` prompt and still requires a
    fresh unique shell-probe response before callers can submit a real command.
    Thus an installer plan is never typed at a boot-loader prompt.
    """
    if timeout <= 0:
        raise QmpError("serial shell-readiness timeout must be positive")
    deadline = time.monotonic() + timeout
    received = bytearray()
    saw_userspace = False
    wake_sent = False
    # A late client sees no historical serial output.  Do not wait for the
    # caller's full command timeout before asking the current console to redraw
    # its prompt, but leave enough time for an in-progress boot to emit normal
    # userspace evidence first.
    silent_attach_wakeup = time.monotonic() + min(0.25, max(timeout / 4, 0.01))

    def emit(chunk: bytes) -> None:
        if on_output is not None:
            on_output(chunk.decode("utf-8", errors="replace"))

    while time.monotonic() < deadline:
        try:
            chunk = connection.recv(4096)
        except TimeoutError:
            if not wake_sent and (saw_userspace or time.monotonic() >= silent_attach_wakeup):
                # This is never a workload command: it only redraws whichever
                # console owns serial.  A boot-loader response is explicitly
                # rejected below, and the unique shell probe remains mandatory
                # before the caller's actual command is submitted.
                connection.sendall(SERIAL_PROMPT_WAKEUP)
                wake_sent = True
            continue
        if not chunk:
            raise QmpError("serial connection closed before MattOS userspace shell became ready")
        received.extend(chunk)
        emit(chunk)
        if serial_console_is_grub(received):
            raise QmpError(
                "GRUB owns the serial console; refusing to submit MattOS shell commands. "
                "The guest did not auto-boot into userspace."
            )
        saw_userspace = saw_userspace or any(signature in received for signature in LIVE_USERSPACE_SIGNATURES)
        if not serial_console_has_shell_prompt(received):
            continue

        marker = f"__MATTOS_LIVE_SHELL_{time.monotonic_ns()}__"
        connection.sendall(shell_probe_payload(marker))
        while time.monotonic() < deadline:
            try:
                probe_chunk = connection.recv(4096)
            except TimeoutError:
                continue
            if not probe_chunk:
                raise QmpError("serial connection closed while verifying the MattOS shell")
            received.extend(probe_chunk)
            emit(probe_chunk)
            if serial_console_is_grub(received):
                raise QmpError("GRUB took ownership of serial while verifying the MattOS shell")
            if shell_probe_satisfied(received, marker):
                return
        raise QmpError("MattOS shell prompt did not answer its unique readiness probe")
    raise QmpError("MattOS userspace shell did not become ready before the readiness deadline")


def serial_command_stream(
    socket_path: Path,
    command: str,
    timeout: float,
    *,
    on_output: Callable[[str], None] | None = None,
    progress_probe: Callable[[], bool] | None = None,
    heartbeat_seconds: float = 15.0,
) -> str:
    """Run a serial command while streaming output and detecting real stalls.

    ``timeout`` is an *idle-progress* limit, never a total wall-clock limit.
    Output from the guest resets it.  Callers running long destructive work can
    also provide a probe (for example target-disk growth) that resets it only
    when an independently observable operation made progress.
    """
    if timeout <= 0:
        raise QmpError("serial idle timeout must be positive")
    marker = f"__MATTOS_TEST_DONE_{time.monotonic_ns()}__"
    payload = serial_command_payload(command, marker)
    connect_deadline = time.monotonic() + timeout
    last_progress = time.monotonic()
    last_heartbeat = last_progress
    received = bytearray()

    def emit(chunk: bytes) -> None:
        if on_output is not None:
            on_output(chunk.decode("utf-8", errors="replace"))

    with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as connection:
        connection.settimeout(min(timeout, 1.0))
        while True:
            try:
                connection.connect(str(socket_path))
                break
            except (FileNotFoundError, ConnectionRefusedError):
                if time.monotonic() >= connect_deadline:
                    raise QmpError(f"serial socket did not become ready: {socket_path}")
                time.sleep(0.1)
        wait_for_live_shell(connection, timeout, on_output=on_output)
        connection.sendall(payload)
        while True:
            try:
                chunk = connection.recv(4096)
            except TimeoutError:
                now = time.monotonic()
                if progress_probe is not None and progress_probe():
                    last_progress = now
                    if on_output is not None:
                        on_output("[serial] target activity observed; extending progress window\n")
                if now - last_heartbeat >= heartbeat_seconds:
                    if on_output is not None:
                        on_output(
                            "[serial] waiting for guest command progress "
                            f"({now - last_progress:.0f}s since last activity)\n"
                        )
                    last_heartbeat = now
                if now - last_progress >= timeout:
                    raise QmpError(
                        f"serial command made no guest-output or externally observable progress "
                        f"for {timeout:g}s: {command!r}"
                    )
                continue
            if not chunk:
                break
            received.extend(chunk)
            emit(chunk)
            last_progress = time.monotonic()
            exit_code = completion_exit_code(received, marker)
            if exit_code is not None:
                output = received.decode("utf-8", errors="replace")
                if exit_code != 0:
                    raise QmpError(f"guest command failed ({marker}:{exit_code}):\n{output}")
                return output
    raise QmpError(f"serial connection closed before command completion: {command!r}")


def serial_command_payload(command: str, marker: str) -> bytes:
    """Encode a shell command for QEMU's raw serial-console chardev.

    The host socket is not a terminal.  Send an explicit carriage return for
    one complete shell compound command with a CRLF submit sequence.  The
    completion frame must not be queued as a second input line: a long-running
    program can otherwise read it from stdin before the interactive shell gets
    it, leaving the host waiting forever for a marker that was never printed.
    A bare socket newline is not reliable across serial-console configurations.
    """
    script = f"( {command} ); rc=$?; printf '\\n{marker}:%s\\n' \"$rc\""
    # Some QEMU serial backends display a lone CR but do not reliably submit
    # it to the guest line discipline under load.  CRLF is the terminal's
    # canonical Enter sequence; the trailing LF is only an empty shell line.
    return (script + "\r\n").encode("utf-8")


def completion_exit_code(output: bytes, marker: str) -> int | None:
    """Return the exact command completion frame's exit status, if present.

    The token is fresh per command, so text echoed from a previous command or
    a prompt cannot satisfy this parser.  The explicit numeric suffix keeps an
    echoed wrapper from looking like completion.  ``\r``, ``\n``, and prompt
    text around the frame are intentionally irrelevant.
    """
    match = re.search(re.escape(marker.encode("utf-8")) + rb":(-?\d+)(?:\r\n|\n|\r|$)", output)
    return int(match.group(1)) if match is not None else None


def serial_command(socket_path: Path, command: str, timeout: float) -> str:
    """Run one serial command, retaining the historical buffered API."""
    return serial_command_stream(socket_path, command, timeout)


def monitor(client: QmpClient, command: str) -> str:
    result = client.execute("human-monitor-command", {"command-line": command})
    return result if isinstance(result, str) else str(result)


def send_key(client: QmpClient, key: str) -> None:
    # QEMU's HMP key syntax is stable and maps directly to its virtual input
    # device. It is sent through the authenticated local QMP connection.
    monitor(client, f"sendkey {key}")


_SHIFTED_QCODES = {
    "!": "1", "@": "2", "#": "3", "$": "4", "%": "5", "^": "6", "&": "7", "*": "8", "(": "9", ")": "0",
    "_": "minus", "+": "equal", "{": "bracket_left", "}": "bracket_right", "|": "backslash",
    ":": "semicolon", '"': "apostrophe", "<": "comma", ">": "dot", "?": "slash",
}
_QCODES = {
    " ": "spc", "\n": "ret", "-": "minus", "=": "equal", "[": "bracket_left", "]": "bracket_right",
    "\\": "backslash", ";": "semicolon", "'": "apostrophe", ",": "comma", ".": "dot", "/": "slash",
}


def qcode_events_for_text(value: str) -> list[dict[str, Any]]:
    events: list[dict[str, Any]] = []
    for character in value:
        shifted = character in _SHIFTED_QCODES or character.isupper()
        lowered = character.lower()
        qcode = _SHIFTED_QCODES.get(character) or _QCODES.get(lowered) or lowered
        if not (len(qcode) == 1 and qcode.isalnum()) and qcode not in _QCODES.values() and qcode not in _SHIFTED_QCODES.values():
            raise QmpError(f"cannot type unsupported character through QMP: {character!r}")
        if shifted:
            events.append({"type": "key", "data": {"down": True, "key": {"type": "qcode", "data": "shift"}}})
        events.append({"type": "key", "data": {"down": True, "key": {"type": "qcode", "data": qcode}}})
        events.append({"type": "key", "data": {"down": False, "key": {"type": "qcode", "data": qcode}}})
        if shifted:
            events.append({"type": "key", "data": {"down": False, "key": {"type": "qcode", "data": "shift"}}})
    return events


def ppm_dimensions(path: Path) -> tuple[int, int]:
    """Read the dimensions from a QMP screendump without decoding pixels."""
    with path.open("rb") as image:
        magic = image.readline().strip()
        if magic not in {b"P3", b"P6"}:
            raise QmpError(f"QMP screendump is not a PPM image: {path}")
        tokens: list[bytes] = []
        while len(tokens) < 2:
            line = image.readline()
            if not line:
                raise QmpError(f"QMP screendump has no dimensions: {path}")
            if line.startswith(b"#"):
                continue
            tokens.extend(line.split())
        try:
            width, height = int(tokens[0]), int(tokens[1])
        except ValueError as exc:
            raise QmpError(f"QMP screendump has invalid dimensions: {path}") from exc
        if width <= 0 or height <= 0:
            raise QmpError(f"QMP screendump has non-positive dimensions: {path}")
        return width, height


def display_size(client: QmpClient) -> tuple[int, int]:
    """Capture a temporary host-side screendump to establish QEMU coordinates."""
    with tempfile.TemporaryDirectory(prefix="mattos-qmp-") as temporary:
        path = Path(temporary) / "display.ppm"
        monitor(client, f"screendump {path}")
        if not path.is_file():
            raise QmpError("QEMU did not create a display screendump")
        return ppm_dimensions(path)


def click(client: QmpClient, x: int, y: int, width: int, height: int) -> None:
    if not 0 <= x < width or not 0 <= y < height:
        raise QmpError(f"click ({x}, {y}) is outside {width}x{height}")
    # QMP's absolute tablet axes are 0..32767, while screendump coordinates
    # are pixels. Scale against the dimensions captured from this guest, not
    # a host/window-size guess. HMP mouse_move is relative and therefore
    # cannot safely represent a screenshot coordinate.
    absolute_x = round(x * 32767 / max(width - 1, 1))
    absolute_y = round(y * 32767 / max(height - 1, 1))
    client.execute(
        "input-send-event",
        {"events": [
            {"type": "abs", "data": {"axis": "x", "value": absolute_x}},
            {"type": "abs", "data": {"axis": "y", "value": absolute_y}},
        ]},
    )
    client.execute(
        "input-send-event",
        {"events": [{"type": "btn", "data": {"button": "left", "down": True}}]},
    )
    client.execute(
        "input-send-event",
        {"events": [{"type": "btn", "data": {"button": "left", "down": False}}]},
    )


def parse_screen(value: str) -> tuple[int, int]:
    try:
        width_text, height_text = value.lower().split("x", 1)
        width, height = int(width_text), int(height_text)
    except ValueError as exc:
        raise argparse.ArgumentTypeError("screen size must be WIDTHxHEIGHT") from exc
    if width <= 0 or height <= 0:
        raise argparse.ArgumentTypeError("screen dimensions must be positive")
    return width, height


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Use a local QMP socket for MattOS graphical test control")
    parser.add_argument("--socket", type=Path, default=DEFAULT_SOCKET, help="QMP socket from run_qemu.py --test-control")
    parser.add_argument("--serial-socket", type=Path, default=DEFAULT_SERIAL_SOCKET, help="serial socket from run_qemu.py --test-control")
    parser.add_argument("--timeout", type=float, default=QMP_TIMEOUT_SECONDS, help="connection/command timeout in seconds")
    commands = parser.add_subparsers(dest="command", required=True)
    commands.add_parser("status", help="print QEMU status")
    commands.add_parser("wait", help="wait until the QMP socket accepts commands")
    screenshot = commands.add_parser("screenshot", help="capture the visible QEMU display as a PPM image")
    screenshot.add_argument("path", type=Path)
    key = commands.add_parser("key", help="send one QEMU key chord, such as ctrl-alt-t")
    key.add_argument("key")
    text = commands.add_parser("text", help="type ASCII text through the virtual keyboard")
    text.add_argument("text")
    click_parser = commands.add_parser("click", help="click a display coordinate through the USB tablet")
    click_parser.add_argument("x", type=int)
    click_parser.add_argument("y", type=int)
    click_parser.add_argument(
        "--screen",
        type=parse_screen,
        default=None,
        metavar="WIDTHxHEIGHT",
        help="display size; defaults to the dimensions of a live QMP screendump",
    )
    serial = commands.add_parser("serial", help="run one command through the existing guest serial shell")
    serial.add_argument("shell_command")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    if args.timeout <= 0:
        raise QmpError("--timeout must be positive")
    socket_path = args.socket.resolve()
    if args.command == "wait":
        wait_for_socket(socket_path, args.timeout)
        print(f"ready: {socket_path}")
        return 0
    if args.command == "serial":
        print(serial_command(args.serial_socket.resolve(), args.shell_command, args.timeout), end="")
        return 0
    with QmpClient(socket_path, args.timeout) as client:
        if args.command == "status":
            print(json.dumps(client.execute("query-status"), sort_keys=True))
        elif args.command == "screenshot":
            path = args.path.resolve()
            path.parent.mkdir(parents=True, exist_ok=True)
            # Keep ordinary GUI capture independent of the guest screenshot
            # portal. QMP writes the display surface directly on the host,
            # which makes this command suitable for basic input/capture
            # harness checks even when portal interaction is the subject of a
            # separate test.
            monitor(client, f"screendump {path}")
            if not path.is_file() or path.stat().st_size == 0:
                raise QmpError(f"QEMU did not create screenshot: {path}")
            print(path)
        elif args.command == "key":
            send_key(client, args.key)
        elif args.command == "text":
            client.execute("input-send-event", {"events": qcode_events_for_text(args.text)})
        elif args.command == "click":
            width, height = args.screen or display_size(client)
            click(client, args.x, args.y, width, height)
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, QmpError) as exc:
        print(f"error: {exc}", file=sys.stderr)
        raise SystemExit(1)
