import ast
import base64
import os
import shlex
import subprocess
import sys
from tempfile import TemporaryDirectory
import unittest
from unittest import mock
import zlib
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from qemu_test_control import serial_command_payload
from upstream_validation import installed_upstream_probes
import upstream_validation


class InstalledProbeTransportTests(unittest.TestCase):
    def test_kde_probe_inherits_the_graphical_manager_environment(self):
        payloads = set()
        with TemporaryDirectory() as temporary:
            executable = Path(temporary) / 'systemd-run'
            executable.write_text(
                '#!/bin/sh\n'
                'while [ "$#" -gt 0 ]; do case "$1" in --*) shift;; *) break;; esac; done\n'
                'export MATTOS_TEST_DESKTOP_ENV=present\nexec "$@"\n'
            )
            executable.chmod(0o755)
            fixture = 'import os; assert os.environ["MATTOS_TEST_DESKTOP_ENV"] == "present"; print("desktop environment inherited")'
            environment = {**os.environ, 'PATH': temporary + os.pathsep + os.environ['PATH']}
            environment.pop('MATTOS_TEST_DESKTOP_ENV', None)
            try:
                with mock.patch.object(upstream_validation, 'KDE_STACK', fixture):
                    for name, command in installed_upstream_probes():
                        if not name.startswith('kde-stack-runtime'):
                            continue
                        args = shlex.split(command)
                        tree = ast.parse(args[-1])
                        payloads.update(Path(node.args[0].value) for node in ast.walk(tree)
                                        if isinstance(node, ast.Call) and isinstance(node.func, ast.Name)
                                        and node.func.id == 'Path')
                        result = subprocess.run(args, env=environment, text=True, capture_output=True, check=True)
                        if name == 'kde-stack-runtime':
                            self.assertIn('desktop environment inherited', result.stdout)
            finally:
                for payload in payloads:
                    payload.unlink(missing_ok=True)

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
                                        'archive-formats', 'gstreamer-pipeline', 'gnupg-signatures', 'kde-stack-runtime'])
        finally:
            for payload in files:
                payload.unlink(missing_ok=True)
