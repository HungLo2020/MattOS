"""Run under the graphical user manager; work only in a disposable directory."""
import os
from pathlib import Path
import subprocess
import tempfile

source = Path(__file__).resolve().parent
with tempfile.TemporaryDirectory(prefix="mattos-desktop-smoke-") as temporary:
    subprocess.run(["cmake", "-S", str(source), "-B", temporary], check=True)
    subprocess.run(["cmake", "--build", temporary, "-j2"], check=True)
    subprocess.run([str(Path(temporary) / "desktop-smoke")], check=True)

def output(*arguments, **kwargs):
    return subprocess.check_output(arguments, text=True, **kwargs).strip()

assert "Fira Mono" in output("fc-match", "-f", "%{family}", "monospace")
assert "Noto Color Emoji" in output("fc-match", "-f", "%{family}", "emoji:charset=1f600")
prompt = output("/bin/brush", "-i", "-c", 'printf "%s" "$PS1"', env=dict(os.environ, TERM="xterm-256color", NO_COLOR=""))
assert r"\e[01;32m" in prompt and r"\e[01;34m" in prompt
plain = output("/bin/brush", "-i", "-c", 'printf "%s" "$PS1"', env=dict(os.environ, TERM="dumb"))
assert r"\e[" not in plain
for command in ("xdg-open", "xdg-mime", "xdg-settings"):
    assert "1.2.1" in output(command, "--version")
assert output("xdg-mime", "query", "default", "text/plain") == "slate.desktop"
for command in ("kde-open", "kioclient", "kmimetypefinder"):
    subprocess.run([command, "--version"], check=True)
print("Fontconfig, interactive prompt, XDG MIME defaults and KDE helpers passed")
