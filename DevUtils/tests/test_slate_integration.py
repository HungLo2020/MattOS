"""Exercise the editor launcher and desktop defaults outside imported source."""
import os
from pathlib import Path
import subprocess
from tempfile import TemporaryDirectory
import unittest

ROOT = Path(__file__).resolve().parents[2]
POLICY = ROOT / "src/system/desktop/editor"


class SlateIntegrationTests(unittest.TestCase):
    def test_visual_launcher_selects_blocking_gui_and_preserves_arguments(self):
        with TemporaryDirectory() as raw:
            root = Path(raw)
            launcher = root / "slate-visual"
            # Redirect only executable locations; run the production shell logic.
            launcher.write_text((POLICY / "slate-visual").read_text().replace("/usr/bin/", f"{root}/"))
            launcher.chmod(0o755)
            capture = root / "capture"
            for binary in ("slate", "slate-gui"):
                stub = root / binary
                stub.write_text('#!/bin/sh\nprintf "%s\\n" "$0" "$@" > "$SLATE_TEST_CAPTURE"\n')
                stub.chmod(0o755)
            cases = (
                ({}, "slate", []),
                ({"WAYLAND_DISPLAY": "wayland-0"}, "slate-gui", ["--wait"]),
                ({"DISPLAY": ":0"}, "slate-gui", ["--wait"]),
                ({"DISPLAY": ":0", "SSH_CONNECTION": "remote session"}, "slate", []),
            )
            base = {k: v for k, v in os.environ.items() if k not in ("DISPLAY", "WAYLAND_DISPLAY", "SSH_CONNECTION")}
            for additions, binary, options in cases:
                subprocess.run([str(launcher), "+23:7", "file with spaces.txt"], check=True,
                               env={**base, **additions, "SLATE_TEST_CAPTURE": str(capture)})
                self.assertEqual(capture.read_text().splitlines(), [str(root / binary), *options, "+23:7", "file with spaces.txt"])
            (root / "slate-gui").unlink()
            subprocess.run([str(launcher), "file.txt"], check=True,
                           env={**base, "WAYLAND_DISPLAY": "wayland-0", "SLATE_TEST_CAPTURE": str(capture)})
            self.assertEqual(capture.read_text().splitlines(), [str(root / "slate"), "file.txt"])

    def test_mime_defaults_are_advertised_by_slate_and_leave_html_to_the_browser(self):
        desktop = (ROOT / "src/userland/slate/packaging/slate.desktop").read_text()
        advertised = set(next(line[9:] for line in desktop.splitlines() if line.startswith("MimeType=")).split(";"))
        defaults = dict(line.split("=", 1) for line in (POLICY / "mimeapps.list").read_text().splitlines() if "=" in line)
        self.assertEqual(defaults["text/plain"], "slate.desktop;")
        self.assertEqual(defaults["application/json"], "slate.desktop;")
        self.assertNotIn("text/html", defaults)
        self.assertTrue(set(defaults) <= advertised)
        self.assertEqual(set(defaults.values()), {"slate.desktop;"})

    def test_login_profile_defaults_and_user_overrides(self):
        profile = ROOT / "src/rootfs/skeleton/etc/profile"
        for chosen in ({}, {"EDITOR": "custom-tui", "VISUAL": "custom-gui"}):
            env = {k: v for k, v in os.environ.items() if k not in ("EDITOR", "VISUAL")}
            output = subprocess.check_output(["/bin/sh", "-c", '. "$1"; printf "%s\\n" "$EDITOR" "$VISUAL"', "sh", str(profile)],
                                             env={**env, **chosen}, text=True)
            self.assertEqual(output.splitlines(), [chosen.get("EDITOR", "/usr/bin/slate"), chosen.get("VISUAL", "/usr/bin/slate-visual")])


if __name__ == "__main__":
    unittest.main()
