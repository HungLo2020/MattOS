#!/usr/bin/env python3
"""Fault-inject a lost live compositor, prove bounded tty recovery, shut down.

Uses the built ISO without altering it; one task-owned disposable VM only.
No kernel/GPU workaround or host display automation dependency is introduced.
"""
import re
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import run_qemu
from qemu_test_control import QmpClient, serial_command, send_key, type_text


def main():
    root = Path(__file__).resolve().parents[2]
    sys.argv = ["run_qemu.py", "--no-build", "--no-install-disk", "--test-control", "--live-media", "usb"]
    args = run_qemu.parse_args()

    def validate(proc, paths):
        deadline = time.monotonic() + 120
        while not paths[0].exists():
            if proc.poll() is not None or time.monotonic() >= deadline:
                raise RuntimeError("QEMU failed before graphics-recovery control setup")
            time.sleep(0.1)
        report = root / "out/logs/graphics-recovery.log"
        output = serial_command(paths[1],
            "for i in $(seq 1 100); do sudo mattos-graphics-startup --check && break; sleep 1; done; "
            "sudo mattos-graphics-startup --check && sudo test -s /run/mattos-graphics/before-cosmic.txt", 240)
        report.write_text(output)
        # Runtime-only fault in the disposable guest. Deliberately no persistent
        # mask/configuration or kernel arguments, and no impact on serial getty.
        output += serial_command(paths[1],
            "export SYSTEMD_PAGER=cat TERM=xterm; sudo systemctl stop cosmic-greeter.service; "
            "sudo pkill -TERM -x cosmic-comp || true; "
            "sudo systemctl restart --no-block mattos-graphics-watchdog.service; "
            "for i in $(seq 1 240); do "
            "systemctl is-active --quiet getty@tty1.service && "
            "test \"$(systemctl show mattos-graphics-watchdog.service -p Result --value)\" != success && break; sleep 1; done; "
            "systemctl is-active getty@tty1.service && "
            "sudo test -s /run/mattos-graphics/report.txt && sudo test -s /run/mattos-graphics/journal.txt && "
            "sudo journalctl -b -u mattos-graphics-watchdog.service -u mattos-graphics-recovery.service --no-pager; "
            "systemctl show mattos-graphics-watchdog.service mattos-graphics-recovery.service "
            "-p ActiveState -p Result -p ExecMainStatus; "
            "cat /sys/class/tty/tty0/active; pgrep -a cosmic-comp || true", 300)
        report.write_text(output)
        if "graphical startup did not establish" not in output:
            raise RuntimeError("watchdog failure/recovery was not observed")
        # Prove the recovered VT accepts actual QMP keyboard input.
        for attempt in range(25):
            with QmpClient(paths[0], 10) as qmp:
                send_key(qmp, "ctrl-c")
                time.sleep(0.15)
                type_text(qmp, "echo MATTOS_RECOVERY_TTY_OK | tee /tmp/recovery-tty\n")
            reply = serial_command(paths[1], "printf '\\n'; cat /tmp/recovery-tty 2>/dev/null || true", 30)
            output += reply
            report.write_text(output)
            if re.search(r"(?m)^MATTOS_RECOVERY_TTY_OK\r?$", reply):
                break
        else:
            raise RuntimeError("recovered text VT did not acknowledge QMP input")
        output += serial_command(paths[1],
            "export SYSTEMD_PAGER=cat TERM=xterm; for i in $(seq 1 30); do systemctl is-active --quiet mattos-graphics-recovery.service || break; sleep 1; done; "
            "test \"$(systemctl show mattos-graphics-recovery.service -p Result --value)\" = success && "
            "test \"$(cat /sys/class/tty/tty0/active)\" = tty1 && "
            "systemctl is-active getty@tty1.service", 60)
        report.write_text(output)
        with QmpClient(paths[0], 10) as qmp:
            qmp.execute("screendump", {"filename": str(root / "out/logs/graphics-recovery.ppm")})
        print(output, flush=True)
        serial_command(paths[1], "sudo systemctl poweroff --no-block", 30)

    result = run_qemu._launch_one(root, run_qemu.ensure_iso_exists(root), args,
                                  boot_iso=True, lifecycle=validate)
    if not result:
        print("PASS: bounded graphics-failure capture and QMP-interactive tty1 recovery; VM exited", flush=True)
    return result


if __name__ == "__main__":
    raise SystemExit(main())
