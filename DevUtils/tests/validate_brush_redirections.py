#!/usr/bin/env python3
"""Small artifact regression: exercise MattOS Brush, not the host shell."""
from pathlib import Path
import subprocess
import tempfile


def main():
    root = Path(__file__).resolve().parents[2]
    command = [str(root / "out/build/glibc/install/lib64/ld-linux-x86-64.so.2"),
               "--library-path", str(root / "out/build/glibc/install/usr/lib/x86_64-linux-gnu"),
               str(root / "out/build/brush/cargo-target/release/brush"), "-c"]
    # This tiny external fixture invokes the same MattOS shell. Keeping its
    # stdout/stderr separate proves descriptor routing across a child boundary.
    import shlex
    child = shlex.join(command + ["printf external"])
    cases = [("printf builtin >&2", b"", b"builtin"),
             (child + " >&2", b"", b"external"),
             ("f() { " + child + "; }; f >&2", b"", b"external"),
             ("value=$(" + child + " >&2); printf '<%s>' \"$value\"", b"<>", b"external"),
             (shlex.join(command + ["printf error >&2"]) + " 2>&1", b"error", b"")]
    with tempfile.TemporaryDirectory(prefix="brush-redirections-") as temporary:
        for script, stdout, stderr in cases:
            result = subprocess.run(command + [script], cwd=temporary, capture_output=True, timeout=15)
            assert (result.returncode, result.stdout, result.stderr) == (0, stdout, stderr), (script, result)
    print("PASS: builtin, external, function, substitution, and reverse descriptor routing")


if __name__ == "__main__":
    main()
