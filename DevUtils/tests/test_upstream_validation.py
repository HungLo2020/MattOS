import ast
import base64
import shlex
import subprocess
import sys
import unittest
import zlib
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from qemu_test_control import serial_command_payload
from upstream_validation import installed_upstream_probes


class InstalledProbeTransportTests(unittest.TestCase):
    def test_uart_transfers_are_bounded_and_reconstruct_executable_probes(self):
        files = set()
        verified = []
        try:
            for name, command in installed_upstream_probes():
                self.assertLess(len(serial_command_payload(command, '__MATTOS_TEST_DONE_1234567890123456__')), 1000, name)
                args = shlex.split(command)
                code = args[-1]
                tree = ast.parse(code)
                path = next(node.args[0].value for node in ast.walk(tree)
                            if isinstance(node, ast.Call) and isinstance(node.func, ast.Name)
                            and node.func.id == 'Path')
                payload = Path(path)
                files.add(payload)
                if '-transfer-' in name:
                    subprocess.run(args, check=True)
                else:
                    source = zlib.decompress(base64.b64decode(payload.read_bytes()))
                    compile(source, name, 'exec')
                    verified.append(name)
                    payload.unlink()
            self.assertEqual(verified, ['glib-gio', 'pixbuf-formats', 'xkb-keymaps',
                                        'archive-formats', 'gstreamer-pipeline', 'gnupg-signatures'])
        finally:
            for payload in files:
                payload.unlink(missing_ok=True)
