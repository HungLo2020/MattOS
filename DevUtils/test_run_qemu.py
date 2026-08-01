#!/usr/bin/env python3
import unittest
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from run_qemu import network_arguments


class QemuNetworkArgumentsTests(unittest.TestCase):
    def test_default_network_is_unprivileged_virtio(self) -> None:
        self.assertEqual(
            network_arguments(False),
            ["-netdev", "user,id=net0", "-device", "virtio-net-pci,netdev=net0"],
        )

    def test_no_network_omits_all_network_arguments(self) -> None:
        self.assertEqual(network_arguments(True), [])


if __name__ == "__main__":
    unittest.main()
