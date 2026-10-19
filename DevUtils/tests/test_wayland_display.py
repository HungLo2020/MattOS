import os
from pathlib import Path
import socket
import subprocess
import sys
from tempfile import TemporaryDirectory
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from run_qemu import installed_wayland_environment


class InstalledWaylandDisplayTests(unittest.TestCase):
    def check_display(self, display, *, regular_file=False):
        with TemporaryDirectory(prefix="mattos-display-") as temporary:
            root = Path(temporary)
            runtime = root / "runtime"
            runtime.mkdir()
            executable = root / "systemctl"
            executable.write_text(
                '#!/bin/sh\n'
                'printf "OTHER=value\\n"\n'
                'if [ -n "$FIXTURE_DISPLAY" ]; then\n'
                '  printf "WAYLAND_DISPLAY=%s\\n" "$FIXTURE_DISPLAY"\n'
                'fi\n'
            )
            executable.chmod(0o755)
            value = str(runtime / "absolute") if display == "absolute" else display
            path = Path(value) if value and value.startswith("/") else runtime / (value or "wayland-0")
            with socket.socket(socket.AF_UNIX) as connection:
                if regular_file:
                    path.write_text("not a Wayland socket")
                else:
                    connection.bind(str(path))
                result = subprocess.run(
                    ["/bin/sh", "-c", installed_wayland_environment() + ' && printf "%s" "$WAYLAND_DISPLAY"'],
                    env={**os.environ, "PATH": str(root) + os.pathsep + os.environ["PATH"],
                         "XDG_RUNTIME_DIR": str(runtime), "FIXTURE_DISPLAY": value or "",
                         "WAYLAND_DISPLAY": "wayland-0"},
                    text=True, capture_output=True, check=False,
                )
            return result, value

    def test_uses_session_socket_including_nonzero_index(self):
        for display in ("wayland-0", "wayland-1"):
            with self.subTest(display=display):
                result, value = self.check_display(display)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(result.stdout, value)

    def test_supports_absolute_session_socket(self):
        result, value = self.check_display("absolute")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout, value)

    def test_rejects_regular_file_in_place_of_socket(self):
        result, _ = self.check_display("wayland-1", regular_file=True)
        self.assertNotEqual(result.returncode, 0)

    def test_missing_session_display_does_not_use_stale_environment_or_zero_socket(self):
        result, _ = self.check_display(None)
        self.assertNotEqual(result.returncode, 0)
