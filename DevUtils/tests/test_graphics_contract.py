"""Physical GPU configuration contracts, not a substitute for AMD hardware tests."""
import pathlib
import subprocess
import os
import socket
import tempfile
import re
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[2]


class GraphicsContractTests(unittest.TestCase):
    def test_amdgpu_display_and_firmware_decompression_enabled(self):
        config = (ROOT / "src/kernel/config/x86_64_mattos.config").read_text().splitlines()
        for symbol in ("DRM_AMDGPU=m", "DRM_AMD_DC=y", "DRM_AMD_DC_FP=y",
                       "FW_LOADER=y", "FW_LOADER_COMPRESS_ZSTD=y",
                       "FW_LOADER_COMPRESS_XZ=y", "DEVTMPFS=y",
                       "MODULE_COMPRESS_ZSTD=y", "MODULE_DECOMPRESS=y"):
            self.assertIn("CONFIG_" + symbol, config)
        self.assertIn("# CONFIG_DEBUG_KERNEL_DC is not set", config)

    def test_mesa_has_physical_amd_and_virtual_drivers(self):
        recipe = (ROOT / "src/tools/mattos-build/src/stages/graphics.rs").read_text()
        self.assertIn('"-Dgallium-drivers=radeonsi,iris,nouveau,virgl,llvmpipe,svga"', recipe)
        self.assertIn('"-Dvulkan-drivers=amd,intel,nouveau,swrast,virtio"', recipe)
        for option in ("-Degl=enabled", "-Dgbm=enabled", "-Dllvm=enabled", "-Dshared-llvm=enabled"):
            self.assertIn(option, recipe)

    def test_common_display_firmware_retained(self):
        firmware = ROOT / "src/system/data/linux-firmware/amdgpu"
        # DCN 2/3 families: Renoir, Rembrandt, RDNA2, Phoenix, RDNA3.
        for name in ("green_sardine_dmcub.bin", "yellow_carp_dmcub.bin",
                     "sienna_cichlid_dmcub.bin", "dcn_3_1_4_dmcub.bin",
                     "dcn_3_2_0_dmcub.bin"):
            self.assertTrue((firmware / name).is_file(), name)

    def test_dcn31_reused_asic_firmware_closure_is_retained(self):
        driver = ROOT / "src/kernel/linux/drivers/gpu/drm/amd/amdgpu"
        declared = set()
        for source in ("gfx_v10_0.c", "psp_v13_0.c", "sdma_v5_2.c"):
            declared.update(re.findall(r'MODULE_FIRMWARE\("(amdgpu/(?:yellow_carp|gc_10_3_[67]|sdma_5_2_[67]|psp_13_0_[358])[^"\n]*)"\)', (driver / source).read_text()))
        self.assertGreaterEqual(len(declared), 27)
        declared.update("amdgpu/" + name for name in ("yellow_carp_dmcub.bin", "dcn_3_1_5_dmcub.bin", "dcn_3_1_6_dmcub.bin"))
        for name in sorted(declared):
            firmware = ROOT / "src/system/data/linux-firmware" / name
            self.assertTrue(firmware.is_file(), name)
            self.assertGreater(firmware.stat().st_size, 0, name)

    def test_report_is_bounded_read_only_and_covers_session_failure(self):
        report = ROOT / "src/system/session/cosmic/mattos-graphics-report"
        subprocess.run(["sh", "-n", str(report)], check=True)
        text = report.read_text()
        for required in ("timeout --kill-after=2s 15s", "device/uevent",
                         "ExecMainStatus", "_COMM=cosmic-comp", "modprobe --show-depends",
                         "ADVERTISED_NOT_PRESENT", "libgallium", "loginctl seat-status"):
            self.assertIn(required, text)
        for forbidden in ("nomodeset", "amdgpu.dc=0", "export LIBGL_ALWAYS_SOFTWARE=", "modprobe -r"):
            self.assertNotIn(forbidden, text)

    def test_diagnostic_entry_is_cli_only_and_preserves_normal_boot(self):
        grub = (ROOT / "src/boot/grub/grub.cfg").read_text()
        normal = grub.split('menuentry "Start MattOS Live" {', 1)[1].split('}', 1)[0]
        self.assertNotIn("drm.debug", normal)
        diagnostic = grub.split('menuentry "MattOS AMD graphics diagnostics (CLI)" {', 1)[1]
        for required in ("mattos.mode=live-cli", "loglevel=4", "drm.debug=0x1ff", "log_buf_len=8M", "amdgpu.dyndbg=+p"):
            self.assertIn(required, diagnostic)
        self.assertNotIn("nomodeset", grub)
        self.assertNotIn("amdgpu.dc=0", grub)

    def test_watchdog_is_bounded_live_only_and_recovers_without_gpu_policy(self):
        directory = ROOT / "src/system/session/cosmic"
        script = (directory / "mattos-graphics-startup").read_text()
        subprocess.run(["sh", "-n", str(directory / "mattos-graphics-startup")], check=True)
        self.assertIn('[ -e /run/mattos-live ] || exit 0', script)
        unit = (directory / "mattos-graphics-watchdog.service").read_text()
        self.assertIn("120s /usr/bin/mattos-graphics-startup --wait", unit)
        self.assertIn("TimeoutStartSec=125s", unit)
        self.assertIn("OnFailure=mattos-graphics-recovery.service", unit)
        self.assertIn("RemainAfterExit=yes", unit)
        self.assertIn("systemctl start getty@tty1.service", script)
        self.assertIn('loginctl activate "$session"', script)
        self.assertLess(script.index("        capture\n"), script.index("systemctl stop cosmic-greeter"))
        for forbidden in ("isolate", "modprobe -r", "amdgpu.dc=0", "LIBGL_ALWAYS_SOFTWARE="):
            self.assertNotIn(forbidden, '\n'.join(l for l in script.splitlines() if not l.lstrip().startswith('#')))

    def test_readiness_requires_real_output_reply_current_mode_and_card(self):
        """Execute the production check function with synthetic /proc + Wayland.

        No VM/host service or GPU is touched. Failed/time-limited CLI replies,
        disabled heads, mismatched head modes and missing card FDs fail closed.
        """
        original = (ROOT / "src/system/session/cosmic/mattos-graphics-startup").read_text()
        function = original.split("enabled_current_mode() (", 1)[1].split("\ncapture() {", 1)[0]
        with tempfile.TemporaryDirectory() as temporary:
            tmp = pathlib.Path(temporary)
            proc = tmp / "proc/123"
            (proc / "fd").mkdir(parents=True)
            (proc / "status").write_text("Uid:\t1000\t1000\t1000\t1000\n")
            runtime = tmp / "run/user/1000"
            runtime.mkdir(parents=True)
            (proc / "environ").write_bytes(f"XDG_RUNTIME_DIR={runtime}\0".encode())
            card = proc / "fd/8"
            card.symlink_to("/dev/dri/card2")
            replies = tmp / "reply"
            bindir = tmp / "bin"
            bindir.mkdir()
            for name, body in {"pgrep": 'if [ "$1" = -u ] && [ "${NO_UI:-0}" = 1 ]; then exit 1; fi; echo 123', "sudo": f'cat "{replies}"; exit "${{PROBE_STATUS:-0}}"'}.items():
                path = bindir / name
                path.write_text("#!/bin/sh\n" + body + "\n")
                path.chmod(0o755)
            script = tmp / "check.sh"
            function = function.replace("/proc/", str(tmp / "proc") + "/").replace("/run/user/", str(tmp / "run/user") + "/")
            script.write_text(f'reports="{tmp}"\nenabled_current_mode() (' + function + "\ncheck_session\n")
            env = {**os.environ, "PATH": str(bindir) + ":/usr/bin:/bin"}
            with socket.socket(socket.AF_UNIX) as wayland:
                wayland.bind(str(runtime / "wayland-1"))
                valid = 'output "DP-2" enabled=#true {\n modes {\n mode 1920 1080 60000 current=#true\n }\n}\n'
                cases = [(valid, 0), ('', 1), (valid.replace('#true', '#false'), 1),
                         (valid.replace('1920', '0'), 1),
                         ('output "DP-1" enabled=#true {\n}\n' + valid.replace('enabled=#true', 'enabled=#false'), 1)]
                for reply, expected in cases:
                    replies.write_text(reply)
                    result = subprocess.run(["sh", str(script)], env=env, capture_output=True, timeout=10)
                    self.assertEqual(result.returncode, expected, result.stderr)
                replies.write_text(valid)
                result = subprocess.run(["sh", str(script)], env={**env, "PROBE_STATUS": "124"}, capture_output=True, timeout=10)
                self.assertEqual(result.returncode, 1)
                result = subprocess.run(["sh", str(script)], env={**env, "NO_UI": "1"}, capture_output=True, timeout=10)
                self.assertEqual(result.returncode, 1)
                card.unlink()
                result = subprocess.run(["sh", str(script)], env=env, capture_output=True, timeout=10)
                self.assertEqual(result.returncode, 1)
