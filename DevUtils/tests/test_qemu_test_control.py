#!/usr/bin/env python3
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import qemu_test_control as control


class QemuTestControlTests(unittest.TestCase):
    def test_text_events_cover_shell_commands_without_guest_automation(self) -> None:
        events = control.qcode_events_for_text("flatpak --user list\n")
        self.assertGreater(len(events), 10)
        self.assertEqual(events[0]["type"], "key")
        self.assertEqual(events[-1]["data"]["key"]["data"], "ret")

    def test_text_events_preserve_shifted_characters(self) -> None:
        events = control.qcode_events_for_text("A!_")
        pressed = [
            event["data"]["key"]["data"]
            for event in events
            if event["data"]["down"]
        ]
        self.assertEqual(pressed, ["shift", "a", "shift", "1", "shift", "minus"])

    def test_unsupported_text_is_rejected_before_input_is_sent(self) -> None:
        with self.assertRaises(control.QmpError):
            control.qcode_events_for_text("emoji 😀")

    def test_screen_size_parser_requires_positive_dimensions(self) -> None:
        self.assertEqual(control.parse_screen("1280x800"), (1280, 800))
        with self.assertRaises(Exception):
            control.parse_screen("0x800")

    def test_ppm_dimensions_match_the_captured_display(self) -> None:
        with tempfile.NamedTemporaryFile(mode="wb") as image:
            image.write(b"P6\n# qmp\n640 480\n255\n")
            image.flush()
            self.assertEqual(control.ppm_dimensions(Path(image.name)), (640, 480))

    def test_click_scales_screendump_pixels_to_absolute_qmp_axes(self) -> None:
        client = mock.Mock()
        control.click(client, 300, 145, 640, 480)
        self.assertEqual(client.execute.call_args_list[0].args[0], "input-send-event")
        events = client.execute.call_args_list[0].args[1]["events"]
        self.assertEqual(events[0]["data"]["value"], round(300 * 32767 / 639))
        self.assertEqual(events[1]["data"]["value"], round(145 * 32767 / 479))
        self.assertEqual(len(client.execute.call_args_list), 3)

    def test_serial_subcommand_keeps_command_text_separate_from_subcommand_name(self) -> None:
        with mock.patch.object(sys, "argv", ["qemu_test_control.py", "serial", "uname -a"]):
            args = control.parse_args()
        self.assertEqual(args.command, "serial")
        self.assertEqual(args.shell_command, "uname -a")
