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
from typing import Any


DEFAULT_SOCKET = Path("out/qemu/test-control/qmp.sock")
DEFAULT_SERIAL_SOCKET = Path("out/qemu/test-control/serial.sock")
QMP_TIMEOUT_SECONDS = 10.0


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


def serial_command(socket_path: Path, command: str, timeout: float) -> str:
    """Run one bounded command through the existing automatic serial shell."""
    marker = f"__MATTOS_TEST_DONE_{time.monotonic_ns()}__"
    payload = f"{command}\nprintf '\\n{marker}:%s\\n' \"$?\"\n".encode("utf-8")
    deadline = time.monotonic() + timeout
    received = bytearray()
    with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as connection:
        connection.settimeout(min(timeout, 1.0))
        while True:
            try:
                connection.connect(str(socket_path))
                break
            except (FileNotFoundError, ConnectionRefusedError):
                if time.monotonic() >= deadline:
                    raise QmpError(f"serial socket did not become ready: {socket_path}")
                time.sleep(0.1)
        # Give the automatic-login getty a bounded opportunity to print its
        # shell prompt before emitting the command.
        while time.monotonic() < deadline:
            try:
                chunk = connection.recv(4096)
            except TimeoutError:
                break
            if not chunk:
                break
            received.extend(chunk)
            if b"$ " in received:
                break
        connection.sendall(payload)
        encoded_marker = marker.encode("utf-8")
        while time.monotonic() < deadline:
            try:
                chunk = connection.recv(4096)
            except TimeoutError:
                continue
            if not chunk:
                break
            received.extend(chunk)
            if encoded_marker in received:
                output = received.decode("utf-8", errors="replace")
                completed = re.findall(rf"{re.escape(marker)}:(-?\d+)", output)
                # The terminal echoes the printf command before it executes;
                # only a marker followed by a numeric exit status is proof
                # that the guest shell finished the requested command.
                if not completed:
                    continue
                if completed[-1] != "0":
                    raise QmpError(f"guest command failed ({marker}:{completed[-1]}):\n{output}")
                return output
    raise QmpError(f"serial command did not finish within {timeout:g}s: {command!r}")


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
