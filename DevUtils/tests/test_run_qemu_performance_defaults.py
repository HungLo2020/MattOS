#!/usr/bin/env python3
import io
import sys
import unittest
from contextlib import redirect_stdout
from pathlib import Path
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import run_qemu


class QemuPerformanceDefaultsTests(unittest.TestCase):
    def test_default_vm_resources_are_desktop_usable(self) -> None:
        with mock.patch.object(sys, "argv", ["run_qemu.py"]):
            args = run_qemu.parse_args()
        self.assertEqual(args.cpus, 4)
        self.assertEqual(args.memory, 6144)

    def test_kvm_selection_reports_hardware_acceleration(self) -> None:
        with mock.patch("run_qemu.Path.exists", return_value=True), mock.patch(
            "run_qemu.os.access", return_value=True
        ):
            arguments, status = run_qemu.acceleration_selection()
        self.assertEqual(arguments, ["-enable-kvm", "-cpu", "host"])
        self.assertEqual(status, "KVM hardware acceleration")

    def test_missing_kvm_reports_tcg_fallback_reason(self) -> None:
        with mock.patch("run_qemu.Path.exists", return_value=False):
            arguments, status = run_qemu.acceleration_selection()
        self.assertEqual(arguments, [])
        self.assertIn("TCG software emulation", status)
        self.assertIn("/dev/kvm is missing", status)

    def test_inaccessible_kvm_reports_tcg_fallback_reason(self) -> None:
        with mock.patch("run_qemu.Path.exists", return_value=True), mock.patch(
            "run_qemu.os.access", return_value=False
        ):
            arguments, status = run_qemu.acceleration_selection()
        self.assertEqual(arguments, [])
        self.assertIn("TCG software emulation", status)
        self.assertIn("not readable/writable", status)

    def test_no_kvm_option_reports_intentional_tcg_fallback(self) -> None:
        arguments, status = run_qemu.acceleration_selection(disabled=True)
        self.assertEqual(arguments, [])
        self.assertIn("TCG software emulation", status)
        self.assertIn("--no-kvm requested", status)

    def test_terminal_warning_is_obvious_for_tcg(self) -> None:
        args = type("Args", (), {"cpus": 4, "memory": 6144})()
        output = io.StringIO()
        with redirect_stdout(output):
            run_qemu.report_launch_configuration(
                args,
                "TCG software emulation (/dev/kvm is missing)",
            )
        text = output.getvalue()
        self.assertIn("4 vCPU(s), 6144 MiB RAM", text)
        self.assertIn("WARNING", text)
        self.assertIn("TCG software emulation", text)
        self.assertIn("very slow", text)

    def test_terminal_status_is_clear_for_kvm(self) -> None:
        args = type("Args", (), {"cpus": 4, "memory": 6144})()
        output = io.StringIO()
        with redirect_stdout(output):
            run_qemu.report_launch_configuration(args, "KVM hardware acceleration")
        text = output.getvalue()
        self.assertIn("4 vCPU(s), 6144 MiB RAM", text)
        self.assertIn("KVM hardware acceleration", text)
        self.assertNotIn("WARNING", text)


if __name__ == "__main__":
    unittest.main()
