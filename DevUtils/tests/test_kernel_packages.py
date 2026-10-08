"""Kernel maintainer scripts must work in offline roots and retain fallbacks."""
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
SCRIPTS = ROOT / 'src/kernel/packaging'


class KernelPackageTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory(prefix='mattos kernel ')
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        (self.root / 'boot').mkdir()
        (self.root / 'usr/lib/mattos').mkdir(parents=True)
        for release in ('7.2.8-mattos', '7.2.9-mattos'):
            (self.root / f'boot/vmlinuz-{release}').write_bytes(b'kernel')
            (self.root / f'boot/initrd.img-{release}').write_bytes(b'initramfs')
        (self.root / 'usr/lib/mattos/kernel-release').write_text('7.2.9-mattos\n')

    def run_script(self, name, release, action):
        script = (SCRIPTS / name).read_text().replace('@KERNEL_RELEASE@', release)
        return subprocess.run(['sh', '-eu', '-c', script, name, action],
                              env={**os.environ, 'DPKG_ROOT': str(self.root)},
                              text=True, capture_output=True)

    def test_upgrade_selects_new_image_and_initramfs_and_keeps_old_files(self):
        (self.root / 'boot/vmlinuz').symlink_to('vmlinuz-7.2.8-mattos')
        (self.root / 'boot/installed-initramfs.cpio.xz').symlink_to('initrd.img-7.2.8-mattos')
        for _ in range(2):
            result = self.run_script('postinst', '7.2.9-mattos', 'configure')
            self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(os.readlink(self.root / 'boot/vmlinuz'), 'vmlinuz-7.2.9-mattos')
        self.assertEqual(os.readlink(self.root / 'boot/installed-initramfs.cpio.xz'), 'initrd.img-7.2.9-mattos')
        self.assertTrue((self.root / 'boot/vmlinuz-7.2.8-mattos').is_file())
        self.assertEqual(self.run_script('postinst', '7.2.8-mattos', 'configure').returncode, 0)
        self.assertEqual(os.readlink(self.root / 'boot/vmlinuz'), 'vmlinuz-7.2.9-mattos')

    def test_missing_initramfs_fails_before_changing_default(self):
        (self.root / 'boot/initrd.img-7.2.9-mattos').unlink()
        result = self.run_script('postinst', '7.2.9-mattos', 'configure')
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse((self.root / 'boot/vmlinuz').is_symlink())

    def test_removal_only_unlinks_aliases_for_the_removed_image(self):
        self.assertEqual(self.run_script('postinst', '7.2.9-mattos', 'configure').returncode, 0)
        self.assertEqual(self.run_script('postrm', '7.2.8-mattos', 'remove').returncode, 0)
        self.assertTrue((self.root / 'boot/vmlinuz').is_symlink())
        self.assertEqual(self.run_script('postrm', '7.2.9-mattos', 'upgrade').returncode, 0)
        self.assertTrue((self.root / 'boot/vmlinuz').is_symlink())
        self.assertEqual(self.run_script('postrm', '7.2.9-mattos', 'remove').returncode, 0)
        self.assertFalse((self.root / 'boot/vmlinuz').is_symlink())
        self.assertFalse((self.root / 'boot/installed-initramfs.cpio.xz').is_symlink())


if __name__ == '__main__':
    unittest.main()
