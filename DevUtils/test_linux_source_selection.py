#!/usr/bin/env python3
from __future__ import annotations

import importlib.util
from pathlib import Path
import subprocess
import tempfile
import unittest


MODULE_PATH = Path(__file__).with_name("test_vendored_source_provenance.py")
SPEC = importlib.util.spec_from_file_location("vendored_source_provenance", MODULE_PATH)
assert SPEC and SPEC.loader
provenance = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(provenance)


class LinuxSourceSelectionTests(unittest.TestCase):
    def setUp(self) -> None:
        self.policy = {
            "retain_arch_root_files": True,
            "retained_architectures": ["x86", "arm64", "riscv", "um"],
            "retained_arch_paths": [
                "arm/crypto/Kconfig",
                "powerpc/crypto/Kconfig",
                "s390/crypto/Kconfig",
                "sparc/crypto/Kconfig",
            ],
            "x86_excluded_paths": ["kernel/entry_32.S"],
        }

    def test_policy_is_limited_to_architectures_and_explicit_x86_32_leaves(self) -> None:
        self.assertTrue(provenance.source_selection_retains(self.policy, "drivers/net/example.c"))
        self.assertTrue(provenance.source_selection_retains(self.policy, "arch/Kconfig"))
        self.assertTrue(provenance.source_selection_retains(self.policy, "arch/x86/kernel/shared.c"))
        self.assertTrue(provenance.source_selection_retains(self.policy, "arch/arm64/kernel/head.S"))
        self.assertTrue(provenance.source_selection_retains(self.policy, "arch/riscv/Kconfig"))
        self.assertTrue(provenance.source_selection_retains(self.policy, "arch/um/Makefile"))
        self.assertTrue(provenance.source_selection_retains(self.policy, "arch/x86/tools/relocs_32.c"))
        self.assertTrue(provenance.source_selection_retains(self.policy, "arch/x86/entry/syscalls/syscall_32.tbl"))
        self.assertTrue(provenance.source_selection_retains(self.policy, "arch/x86/um/bugs_32.c"))
        self.assertTrue(provenance.source_selection_retains(self.policy, "arch/arm/crypto/Kconfig"))
        self.assertFalse(provenance.source_selection_retains(self.policy, "arch/arm/Kconfig"))
        self.assertFalse(provenance.source_selection_retains(self.policy, "arch/arm/crypto/aes-ce-core.S"))
        self.assertFalse(provenance.source_selection_retains(self.policy, "arch/x86/kernel/entry_32.S"))

    def test_audit_rejects_missing_retained_and_stale_excluded_paths(self) -> None:
        with tempfile.TemporaryDirectory() as raw_root:
            root = Path(raw_root)
            subprocess.run(["git", "init", "-q"], cwd=root, check=True)
            source_root = root / "src/kernel/linux"
            stale = source_root / "arch/arm/Kconfig"
            stale.parent.mkdir(parents=True)
            stale.write_text("stale excluded architecture\n")

            component = {"name": "linux", "path": "src/kernel/linux"}
            entries = [("100644", "blob", "0" * 40, "arch/x86/Kconfig")]
            state = {
                "upstream_tree": "1" * 40,
                "imported_tree_digest": provenance.imported_digest(entries),
                "imported_tree_digest_algorithm": provenance.SELECTED_IMPORTED_DIGEST_ALGORITHM,
            }
            original_root = provenance.ROOT
            provenance.ROOT = root
            try:
                _, failures = provenance.verify_component_tree(
                    component,
                    state,
                    "1" * 40,
                    entries,
                    {},
                    self.policy,
                )
            finally:
                provenance.ROOT = original_root

            self.assertIn("missing upstream path arch/x86/Kconfig", failures)
            self.assertIn("stale source-selection-excluded path arch/arm/Kconfig", failures)


if __name__ == "__main__":
    unittest.main()
