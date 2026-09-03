#!/usr/bin/env python3
import re
import sys
import tempfile
import unittest
from collections import deque
from pathlib import Path
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import qemu_test_control as control


class _FakeSerialSocket:
    def __init__(self, chunks: list[bytes], *, wake_prompt: bool = False, wake_response: bytes | None = None) -> None:
        self.chunks = deque(chunks)
        self.sent: list[bytes] = []
        self.wake_prompt = wake_prompt
        self.wake_response = wake_response

    def __enter__(self) -> "_FakeSerialSocket":
        return self

    def __exit__(self, *_: object) -> None:
        return None

    def settimeout(self, _timeout: float) -> None:
        return None

    def connect(self, _path: str) -> None:
        return None

    def sendall(self, payload: bytes) -> None:
        self.sent.append(payload)
        if payload == control.SERIAL_PROMPT_WAKEUP:
            if self.wake_response is not None:
                self.chunks.append(self.wake_response)
            elif self.wake_prompt:
                self.chunks.append(b"\r\nmattos@MattOS:~$ ")
        elif b"__MATTOS_LIVE_SHELL_" in payload:
            marker = payload.split(b"\\n")[1]
            self.chunks.append(b"\r\n" + marker + b"\r\nmattos@MattOS:~$ ")
        elif b"__MATTOS_TEST_DONE_" in payload:
            marker = re.search(rb"(__MATTOS_TEST_DONE_[A-Za-z0-9_]+__)", payload).group(1)
            self.chunks.append(b"\r\n" + marker + b":0\r\n")

    def recv(self, _size: int) -> bytes:
        if self.chunks:
            return self.chunks.popleft()
        raise TimeoutError


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

    def test_serial_payload_is_one_compound_line_with_a_completion_frame(self) -> None:
        payload = control.serial_command_payload("printf test", "__DONE__")
        self.assertTrue(payload.endswith(b"\r\n"))
        self.assertEqual(payload.count(b"\r"), 1)
        self.assertEqual(payload.count(b"\n"), 1)
        self.assertIn(b"( printf test ); rc=$?; printf '\\n__DONE__:%s\\n' \"$rc\"\r\n", payload)

    def test_completion_parser_accepts_all_serial_line_endings_and_prompts(self) -> None:
        marker = "__MATTOS_TEST_DONE_new__"
        for ending in (b"\r", b"\n", b"\r\n"):
            self.assertEqual(
                control.completion_exit_code(b"prompt$ " + marker.encode() + b":0" + ending + b"prompt$ ", marker),
                0,
            )

    def test_completion_parser_requires_the_current_marker_and_numeric_status(self) -> None:
        self.assertIsNone(control.completion_exit_code(b"__MATTOS_TEST_DONE_old__:0\r\n", "__MATTOS_TEST_DONE_new__"))
        self.assertIsNone(control.completion_exit_code(b"__MATTOS_TEST_DONE_new__:not-a-status\r\n", "__MATTOS_TEST_DONE_new__"))
        self.assertEqual(control.completion_exit_code(b"__MATTOS_TEST_DONE_new__:17\r\n", "__MATTOS_TEST_DONE_new__"), 17)

    def test_command_returns_immediately_when_marker_is_last_output(self) -> None:
        connection = _FakeSerialSocket([b"\r\nmattos@MattOS:~$ "])
        with mock.patch.object(control.socket, "socket", return_value=connection):
            output = control.serial_command_stream(Path("serial.sock"), "true", 1)
        self.assertIn(":0", output)

    def test_nonzero_completion_is_propagated(self) -> None:
        connection = _FakeSerialSocket([b"\r\nmattos@MattOS:~$ "])
        original_sendall = connection.sendall
        def sendall(payload: bytes) -> None:
            original_sendall(payload)
            if b"__MATTOS_TEST_DONE_" in payload:
                connection.chunks.clear()
                marker = re.search(rb"(__MATTOS_TEST_DONE_[A-Za-z0-9_]+__)", payload).group(1)
                connection.chunks.append(marker + b":7\r\n")
        connection.sendall = sendall
        with mock.patch.object(control.socket, "socket", return_value=connection):
            with self.assertRaisesRegex(control.QmpError, "guest command failed"):
                control.serial_command_stream(Path("serial.sock"), "false", 1)

    def test_serial_console_wakeup_is_a_harmless_carriage_return(self) -> None:
        # A fresh socket connection cannot rely on getty replaying an already
        # rendered prompt, so the stream emits this byte before it reads.
        self.assertEqual(control.SERIAL_PROMPT_WAKEUP, b"\r")

    def test_grub_prompt_is_never_considered_a_userspace_shell(self) -> None:
        self.assertTrue(control.serial_console_is_grub(b"\r\ngrub> "))
        self.assertFalse(control.serial_console_has_shell_prompt(b"\r\ngrub> "))

    def test_grub_menu_output_is_not_considered_userspace_ready(self) -> None:
        menu = b"GNU GRUB version 2.12\r\n*MattOS\r\nBooting in 2 seconds\r\n"
        self.assertFalse(control.serial_console_is_grub(menu))
        self.assertFalse(control.serial_console_has_shell_prompt(menu))

    def test_shell_probe_requires_its_new_unique_marker(self) -> None:
        old_marker = "__MATTOS_LIVE_SHELL_old__"
        new_marker = "__MATTOS_LIVE_SHELL_new__"
        self.assertTrue(control.serial_console_has_shell_prompt(b"\r\nmattos@MattOS:~$ "))
        self.assertTrue(control.shell_probe_satisfied(b"\r\n" + old_marker.encode() + b"\r\n", old_marker))
        self.assertFalse(control.shell_probe_satisfied(old_marker.encode(), old_marker))
        self.assertFalse(control.shell_probe_satisfied(old_marker.encode(), new_marker))

    def test_brush_osc_integration_prefix_does_not_hide_live_shell_prompt(self) -> None:
        # Brush emits OSC 3008 shell-integration state without a newline just
        # before the visible prompt on the serial console.
        stream = b"\x1b]3008;type=shell\x1b\\mattos@mattos-test:~$ \r"
        self.assertTrue(control.serial_console_has_shell_prompt(stream))

    def test_shell_probe_is_a_harmless_printf_command(self) -> None:
        payload = control.shell_probe_payload("__MATTOS_LIVE_SHELL_probe__")
        self.assertEqual(payload, b"printf '\\n__MATTOS_LIVE_SHELL_probe__\\n'\r")

    def test_command_submission_begins_only_after_unique_shell_probe(self) -> None:
        connection = _FakeSerialSocket([b"systemd[1]: Started\r\n", b"\r\nmattos@MattOS:~$ "])
        with mock.patch.object(control.socket, "socket", return_value=connection):
            control.serial_command_stream(Path("serial.sock"), "true", 1)
        self.assertEqual(len(connection.sent), 2)
        self.assertIn(b"__MATTOS_LIVE_SHELL_", connection.sent[0])
        self.assertIn(b"( true ); rc=$?", connection.sent[1])

    def test_getty_is_woken_only_after_userspace_boot_evidence(self) -> None:
        connection = _FakeSerialSocket([b"systemd[1]: Started serial-getty\r\n"], wake_prompt=True)
        with mock.patch.object(control.socket, "socket", return_value=connection):
            control.wait_for_live_shell(connection, 0.02)
        self.assertEqual(connection.sent[0], control.SERIAL_PROMPT_WAKEUP)
        self.assertIn(b"__MATTOS_LIVE_SHELL_", connection.sent[1])

    def test_late_serial_attach_can_redraw_a_userspace_prompt(self) -> None:
        # QEMU does not replay old serial output to a newly attached socket.
        # A harmless CR is allowed after a bounded silent period, but the
        # subsequent unique marker still proves that a real shell owns serial.
        connection = _FakeSerialSocket([], wake_prompt=True)
        control.wait_for_live_shell(connection, 0.02)
        self.assertEqual(connection.sent[0], control.SERIAL_PROMPT_WAKEUP)
        self.assertIn(b"__MATTOS_LIVE_SHELL_", connection.sent[1])

    def test_late_serial_attach_rejects_grub_after_harmless_wakeup(self) -> None:
        connection = _FakeSerialSocket([], wake_response=b"\r\ngrub> ")
        with self.assertRaisesRegex(control.QmpError, "GRUB owns"):
            control.wait_for_live_shell(connection, 0.02)
        self.assertEqual(connection.sent, [control.SERIAL_PROMPT_WAKEUP])

    def test_grub_prompt_refuses_command_submission(self) -> None:
        connection = _FakeSerialSocket([b"\r\ngrub> "])
        with mock.patch.object(control.socket, "socket", return_value=connection):
            with self.assertRaisesRegex(control.QmpError, "GRUB owns"):
                control.serial_command_stream(Path("serial.sock"), "dangerous-command", 1)
        self.assertEqual(connection.sent, [])
