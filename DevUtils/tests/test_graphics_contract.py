"""Physical GPU configuration contracts, not a substitute for AMD hardware tests."""
import pathlib
import subprocess
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
        report = ROOT / "src/system/session/plasma/mattos-graphics-report"
        subprocess.run(["sh", "-n", str(report)], check=True)
        text = report.read_text()
        for required in ("timeout --kill-after=2s 15s", "device/uevent",
                         "ExecMainStatus", "_COMM=kwin_wayland", "modprobe --show-depends",
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
        directory = ROOT / "src/system/session/plasma"
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
        self.assertLess(script.index("        capture\n"), script.index("systemctl stop plasma-greeter"))
        self.assertIn("pgrep -x kwin_wayland", script)
        self.assertIn("pgrep -u \"$uid\" -x plasmashell", script)
        for forbidden in ("isolate", "modprobe -r", "amdgpu.dc=0", "LIBGL_ALWAYS_SOFTWARE="):
            self.assertNotIn(forbidden, '\n'.join(l for l in script.splitlines() if not l.lstrip().startswith('#')))
