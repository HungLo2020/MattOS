"""Exercise shell startup and the real KWallet environment relay protocol."""
from pathlib import Path
import os
import shlex
import socket
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]


class DesktopDefaultsTests(unittest.TestCase):
    def prompt(self, uid, terminal, *, no_color="", interactive=True):
        script = ROOT / "src/rootfs/skeleton/etc/bash.bashrc"
        command = f'id() {{ printf %s {uid}; }}; PS1=sentinel; . {shlex.quote(str(script))}; printf "%s" "$PS1"'
        options = ["bash", "--noprofile", "--norc"]
        if interactive:
            options.append("-i")
        return subprocess.run(options + ["-c", command],
                              env=dict(os.environ, TERM=terminal, NO_COLOR=no_color),
                              text=True, capture_output=True, check=True).stdout

    def test_prompt_is_colored_and_accounts_are_distinguishable(self):
        self.assertIn(r"\e[01;32m", self.prompt(1000, "xterm-256color"))
        self.assertIn(r"\e[01;31m", self.prompt(0, "linux"))
        self.assertIn(r"\e[01;34m", self.prompt(1000, "xterm-256color"))

    def test_prompt_respects_plain_terminals_opt_out_and_noninteractive_shells(self):
        self.assertNotIn(r"\e[", self.prompt(1000, "dumb"))
        self.assertNotIn(r"\e[", self.prompt(1000, "xterm", no_color="1"))
        self.assertEqual(self.prompt(1000, "xterm", interactive=False), "sentinel")

    def test_wallet_relay_passes_environment_and_closes_stream(self):
        helper = ROOT / "src/system/session/kwallet/pam_kwallet_init"
        with tempfile.TemporaryDirectory() as temporary:
            path = str(Path(temporary) / "wallet.socket")
            with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as server:
                server.bind(path)
                server.listen(1)
                server.settimeout(10)
                environment = {"PAM_KWALLET5_LOGIN": path, "LANG": "de_DE.UTF-8", "XDG_CURRENT_DESKTOP": "KDE", "EXAMPLE": "space and ünicode"}
                process = subprocess.Popen(["python3", str(helper)], env=environment)
                try:
                    with server.accept()[0] as connection:
                        connection.settimeout(10)
                        received = bytearray()
                        while chunk := connection.recv(4096):
                            received.extend(chunk)
                    self.assertEqual(process.wait(timeout=10), 0)
                    values = dict(line.split(b"=", 1) for line in bytes(received).splitlines())
                    for key, value in environment.items():
                        self.assertEqual(values[key.encode()], value.encode())
                finally:
                    if process.poll() is None:
                        process.kill()
                        process.wait()

    def test_wallet_relay_is_a_noop_without_pam_credentials(self):
        helper = ROOT / "src/system/session/kwallet/pam_kwallet_init"
        subprocess.run(["python3", str(helper)], env={}, check=True, timeout=10)


if __name__ == "__main__":
    unittest.main()
