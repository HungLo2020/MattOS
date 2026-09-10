#!/usr/bin/env python3
"""Exercise source-built GRUB relocation, hybrid modules and timestamp policy.

Uses completed stage artifacts only, creates a tiny disposable rescue image,
and never modifies the canonical ISO or host boot configuration.
"""
import os
from pathlib import Path
import shlex
import shutil
import subprocess
import tempfile


def main():
    root = Path(__file__).resolve().parents[2]
    install = root / "out/build/grub/install"
    loader = root / "out/build/glibc/install/lib64/ld-linux-x86-64.so.2"
    libraries = ":".join(str(root / f"out/build/{component}/install/usr/lib/x86_64-linux-gnu")
                         for component in ("glibc", "gcc-runtime", "zlib", "xz"))
    command = [str(loader), "--library-path", libraries, str(install / "usr/bin/grub-mkrescue")]
    environment = {**os.environ, "SOURCE_DATE_EPOCH": "1767225600",
                   "pkglibdir": str(install / "usr/lib/grub"),
                   "pkgdatadir": str(install / "usr/share/grub")}
    temporary_parent = root / "out/tmp"
    temporary_parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="grub-contract-", dir=temporary_parent) as temporary:
        temporary = Path(temporary)
        source = temporary / "tree"
        (source / "boot/grub").mkdir(parents=True)
        (source / "boot/grub/grub.cfg").write_text("menuentry 'GRUB contract test' { halt; }\n")
        image = temporary / "contract.iso"
        environment["TMPDIR"] = str(temporary)
        subprocess.run(command + ["-o", str(image), str(source),
                                  "--modification-date=2026010100000000", "--set_all_file_dates",
                                  "2026010100000000"], env=environment, check=True)
        contents = subprocess.run(["xorriso", "-indev", str(image), "-ls", "/.disk"],
                                  text=True, capture_output=True, check=True)
        assert "2026-01-01-00-00-00-00.uuid" in contents.stdout + contents.stderr, contents
        boot = subprocess.run(["xorriso", "-indev", str(image), "-report_el_torito", "as_mkisofs"],
                              text=True, capture_output=True, check=True)
        text = boot.stdout + boot.stderr
        assert "-b " in text and "-e " in text, text
        # New xorriso versions reject malformed epochs even during their help
        # probe, before mkrescue reaches its own timestamp parser. Isolate that
        # parser here; the successful hybrid-image test above uses real xorriso
        # and the unmodified environment throughout.
        xorriso = shutil.which("xorriso")
        assert xorriso
        probe = temporary / "xorriso-without-epoch"
        probe.write_text("#!/bin/sh\nunset SOURCE_DATE_EPOCH\nexec " + shlex.quote(xorriso) + ' "$@"\n')
        probe.chmod(0o755)
        for epoch in ("bad", "-1", "", "999999999999999999999999"):
            failed = subprocess.run(command + ["--xorriso", str(probe), "-o", str(temporary / "invalid.iso"), str(source)],
                                    env={**environment, "SOURCE_DATE_EPOCH": epoch},
                                    text=True, capture_output=True)
            assert failed.returncode != 0 and "invalid SOURCE_DATE_EPOCH" in failed.stderr, failed
    print("PASS: owned BIOS/EFI modules, deterministic boot UUID, invalid timestamp rejection; fixtures removed")


if __name__ == "__main__":
    main()
