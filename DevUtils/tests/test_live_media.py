"""Host fixtures for early media identity; no mounting or image changes."""
import pathlib
import subprocess
import tempfile
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[2]


class LiveMediaTests(unittest.TestCase):
    def test_image_label_and_boot_layout_match_discovery_contract(self):
        image = (ROOT / "src/tools/mattos-build/src/stages/image.rs").read_text()
        self.assertIn('"-volid"', image)
        self.assertIn('"MATTOS_LIVE"', image)
        grub = (ROOT / "src/boot/grub/grub.cfg").read_text()
        self.assertIn("linux /boot/vmlinuz", grub)
        self.assertIn("initrd /boot/early-initramfs.cpio.xz", grub)
        self.assertNotIn("root=/dev/sr0", grub)

    def test_iso_identity_rejects_unrelated_and_truncated_devices(self):
        with tempfile.TemporaryDirectory() as temporary:
            source = pathlib.Path(temporary) / "test.c"
            source.write_text('''#define main live_init_main
#include "live-init.c"
#undef main
#include <assert.h>
int main(void) {
    unsigned char pvd[2048] = {0};
    assert(!mattos_iso_descriptor(pvd, 2048));
    pvd[0] = 1; pvd[6] = 1;
    memcpy(pvd + 1, "CD001", 5);
    memcpy(pvd + 40, "MATTOS_LIVE                     ", 32);
    assert(mattos_iso_descriptor(pvd, 2048));
    assert(!mattos_iso_descriptor(pvd, 100));
    pvd[40] = 'X'; assert(!mattos_iso_descriptor(pvd, 2048));
    return 0;
}
''')
            executable = pathlib.Path(temporary) / "test"
            subprocess.run(["cc", "-std=c11", "-Wall", "-Wextra", "-Werror",
                            "-I", str(ROOT / "src/boot"), str(source), "-o", str(executable)], check=True)
            subprocess.run([str(executable)], check=True)

    def test_harness_raw_transports_preserve_iso_and_optical_default(self):
        for media in ("optical", "usb", "sata", "nvme"):
            result = subprocess.run(["python3", "DevUtils/run_qemu.py", "--no-build",
                                     "--no-install-disk", "--headless", "--dry-run",
                                     "--live-media", media], cwd=ROOT, capture_output=True, text=True, check=True)
            device = {"optical": "scsi-cd", "usb": "usb-storage", "sata": "ide-hd", "nvme": "nvme,drive"}[media]
            self.assertIn(device, result.stdout)
            self.assertIn("snapshot=on" if media in ("sata", "nvme") else "readonly=on", result.stdout)
