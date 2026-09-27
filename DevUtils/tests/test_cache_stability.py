import os
import subprocess
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import check_cache_stability


class CacheMissParsingTests(unittest.TestCase):
    def test_stage_and_package_misses_are_reported_with_their_reasons(self) -> None:
        output = "\n".join(
            [
                "cache hit: zlib (full input digest matched)",
                "cache miss: expat (input digest changed (dependency output); "
                "changed inputs: dependency:<target-toolchain-v2>)",
                "package cache hit: libexpat1",
                "package cache miss: openssh-server",
                "[build] expat: running (full log: out/logs/expat.log)",
                "the phrase cache miss: inside a log line is not a report",
            ]
        )
        self.assertEqual(
            check_cache_stability.cache_misses(output),
            [
                "cache miss: expat (input digest changed (dependency output); "
                "changed inputs: dependency:<target-toolchain-v2>)",
                "package cache miss: openssh-server",
            ],
        )

    def test_an_all_hit_build_has_no_misses(self) -> None:
        self.assertEqual(
            check_cache_stability.cache_misses("cache hit: a\npackage cache hit: b\n"), []
        )


@unittest.skipUnless(
    os.environ.get("MATTOS_RUN_CACHE_STABILITY_CHECK") == "1",
    "set MATTOS_RUN_CACHE_STABILITY_CHECK=1 to run two full builds around the unit tests",
)
class CacheStabilityIntegrationTests(unittest.TestCase):
    def test_no_op_build_after_unit_tests_reuses_every_cache(self) -> None:
        script = Path(__file__).resolve().parents[1] / "check_cache_stability.py"
        completed = subprocess.run([sys.executable, str(script)], check=False)
        self.assertEqual(completed.returncode, 0)


if __name__ == "__main__":
    unittest.main()
