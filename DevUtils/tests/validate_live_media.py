#!/usr/bin/env python3
"""Boot the built ISO using each transport, validate COSMIC, and shut down.

Run from the repository root. This integration test starts only one VM at a
time and uses run_qemu's normal process/socket cleanup. No ISO rebuilds.
"""
import argparse
import hashlib
import re
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import run_qemu
from qemu_test_control import QmpClient, serial_command, send_key, type_text


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--media", nargs="+", choices=("optical", "usb", "sata", "nvme"),
                        default=["optical", "usb", "sata", "nvme"])
    selected = parser.parse_args().media
    root = Path(__file__).resolve().parents[2]
    iso = run_qemu.ensure_iso_exists(root)
    with iso.open("rb") as image:
        original_digest = hashlib.file_digest(image, "sha256").hexdigest()
    for media in selected:
        sys.argv = ["run_qemu.py", "--no-build", "--no-install-disk", "--test-control",
                    "--live-media", media]
        args = run_qemu.parse_args()
        def validate(proc, paths):
            deadline = time.monotonic() + 120
            while not paths[0].exists():
                if proc.poll() is not None:
                    raise RuntimeError(f"{media}: QEMU exited {proc.returncode} before control socket creation")
                if time.monotonic() >= deadline:
                    raise RuntimeError(f"{media}: QMP socket was not created")
                time.sleep(0.1)
            # One transaction waits for real shell readiness, then records
            # driver/firmware/unit diagnostics before assertions.
            output = serial_command(paths[1],
                "uname -r; cat /run/mattos/live-medium; cat /proc/mounts; "
                "cat /proc/modules; sudo dmesg | tail -120; "
                "systemctl --failed --no-pager; ip address; sudo mattos-graphics-report; "
                "for attempt in $(seq 1 120); do "
                "pgrep -x cosmic-panel >/dev/null && pgrep -x cosmic-bg >/dev/null && break; sleep 1; done; "
                "test -e /run/mattos-live && "
                "systemctl is-active NetworkManager && "
                "systemctl is-active mattos-live-graphical.target && "
                "pgrep -x cosmic-comp && pgrep -x cosmic-session && "
                "pgrep -x cosmic-panel && pgrep -x cosmic-bg && "
                "sudo test -s /run/mattos-graphics/before-cosmic.txt && "
                "sudo mattos-graphics-startup --check && "
                "systemctl is-active mattos-graphics-watchdog.service && "
                "modinfo -F firmware amdgpu >/tmp/amdgpu-firmware && test -s /tmp/amdgpu-firmware", 240)
            report = root / f"out/logs/live-media-{media}.log"
            report.write_text(output)
            if "Report complete. Advertised firmware" not in output:
                raise RuntimeError(f"{media}: packaged graphics diagnostics did not complete")
            with QmpClient(paths[0], 10) as qmp:
                send_key(qmp, "meta_l-t")
            output += serial_command(paths[1],
                "for attempt in $(seq 1 60); do "
                "pgrep -P \"$(pgrep -x cosmic-term | head -1)\" >/dev/null 2>&1 && break; sleep 1; done; "
                "pgrep -a cosmic-term && pgrep -P \"$(pgrep -x cosmic-term | head -1)\"", 120)
            # A shell child can precede surface mapping/focus. Establish GUI
            # readiness by an actual, harmless input acknowledgement, not a
            # sleep or merely the existence of cosmic-term's process.
            for attempt in range(30):
                with QmpClient(paths[0], 10) as qmp:
                    send_key(qmp, "ctrl-c")
                    time.sleep(0.15)  # HMP sendkey releases held keys after 100ms.
                    type_text(qmp, "echo MATTOS_GUI_INPUT_OK | tee /tmp/mattos-gui-input\n")
                acknowledgement = serial_command(paths[1],
                    "printf '\\n'; if test -s /tmp/mattos-gui-input; then cat /tmp/mattos-gui-input; fi", 30)
                output += acknowledgement
                report.write_text(output)
                if re.search(r"(?m)^MATTOS_GUI_INPUT_OK\r?$", acknowledgement):
                    break
            else:
                raise RuntimeError(f"{media}: mapped terminal did not acknowledge QMP keyboard input")
            report.write_text(output)
            with QmpClient(paths[0], 10) as qmp:
                qmp.execute("screendump", {"filename": str(root / f"out/logs/live-media-{media}.ppm")})
            output += serial_command(paths[1],
                "(set -e; test -s /usr/share/libdrm/amdgpu.ids; "
                "dpkg-query -S /usr/share/libdrm/amdgpu.ids; "
                "test ! -e /etc/systemd/system/timers.target.wants/mattos-apt-bootstrap.timer; "
                "for theme in Dark Light; do for suffix in '' .Builder; do "
                "grep -qx false /usr/share/cosmic/com.system76.CosmicTheme.$theme$suffix/v2/frosted_maximized_apps || exit 1; "
                "done; done; "
                "sudo journalctl -b --no-pager -o cat > /tmp/mattos-desktop-policy-journal; "
                "if grep -E 'GetKey.*(frosted_maximized_apps|padding_overlap|keep_style_on_maximize)' /tmp/mattos-desktop-policy-journal; "
                "then exit 1; fi; echo DESKTOP_POLICY_RUNTIME_OK)", 60)
            report.write_text(output)
            print(output, flush=True)
            # A desktop may handle the emulated power button with an
            # interactive confirmation dialog. Request orderly shutdown from
            # the test's authorized serial shell after completing GUI proof.
            serial_command(paths[1], "sudo systemctl poweroff --no-block", 30)
        result = run_qemu._launch_one(root, iso, args,
                                      boot_iso=True, lifecycle=validate)
        if result:
            raise SystemExit(result)
        print(f"PASS: {media} live graphical boot; QEMU exited and control sockets cleaned", flush=True)
    with iso.open("rb") as image:
        final_digest = hashlib.file_digest(image, "sha256").hexdigest()
    if original_digest != final_digest:
        raise RuntimeError("canonical ISO changed during media boot tests")
    print(f"PASS: canonical ISO unchanged, sha256={final_digest}", flush=True)


if __name__ == "__main__":
    main()
