"""Exercise installed XDG tools in disposable homes; never touch the login home."""
import os
from pathlib import Path
import subprocess
import tempfile

UPDATE = "/usr/bin/xdg-user-dirs-update"
LOOKUP = "/usr/bin/xdg-user-dir"
DEFAULTS = ("Desktop", "Downloads", "Templates", "Public", "Documents", "Music", "Pictures", "Videos", "Projects")

with tempfile.TemporaryDirectory(prefix="mattos-user-dirs-") as temporary:
    home = Path(temporary) / "home with spaces"
    home.mkdir()
    config = home / "settings"
    environment = dict(os.environ, HOME=str(home), XDG_CONFIG_HOME=str(config), LC_ALL="C")

    def run(program, *arguments):
        return subprocess.check_output([program, *map(str, arguments)], env=environment, text=True).strip()

    run("/bin/brush", "--login", "-i", "-c", 'test -f "$XDG_CONFIG_HOME/user-dirs.dirs"')
    for name in DEFAULTS:
        assert (home / name).is_dir(), name
        assert (home / name).stat().st_uid == os.getuid(), name
    assert run(LOOKUP, "DOCUMENTS") == str(home / "Documents")
    assert not (home / ".config/user-dirs.dirs").exists(), "XDG_CONFIG_HOME must be respected"
    before = (config / "user-dirs.dirs").read_bytes()
    run(UPDATE)
    assert (config / "user-dirs.dirs").read_bytes() == before, "repeat login changed configuration"

    documents = home / "Work files"
    documents.mkdir()
    sentinel = documents / "keep.txt"
    sentinel.write_text("existing data")
    run(UPDATE, "--set", "DOCUMENTS", documents)
    (home / "Documents").rmdir()
    run(UPDATE, "--set", "DOWNLOAD", home)
    (home / "Downloads").rmdir()
    run(UPDATE)
    assert run(LOOKUP, "DOCUMENTS") == str(documents)
    assert sentinel.read_text() == "existing data"
    assert not (home / "Documents").exists(), "custom path was reset"
    assert Path(run(LOOKUP, "DOWNLOAD")) == home
    assert not (home / "Downloads").exists(), "disabled directory was recreated"

    disabled = Path(temporary) / "disabled"
    disabled.mkdir()
    disabled_config = disabled / ".config"
    disabled_config.mkdir()
    (disabled_config / "user-dirs.conf").write_text("enabled=False\n")
    environment.update(HOME=str(disabled), XDG_CONFIG_HOME=str(disabled_config))
    run(UPDATE)
    assert not (disabled_config / "user-dirs.dirs").exists(), "user opt-out was ignored"
    assert not any((disabled / name).exists() for name in DEFAULTS)

    # Use a private locale tree, leaving the installed system's locale unchanged.
    locale_root = Path(temporary) / "locales"
    locale_root.mkdir()
    run("/usr/bin/localedef", "--no-archive", "-i", "de_DE", "-f", "UTF-8", locale_root / "de_DE.UTF-8")
    german = Path(temporary) / "german"
    german.mkdir()
    environment.update(HOME=str(german), XDG_CONFIG_HOME=str(german / ".config"),
                       LOCPATH=str(locale_root), LANG="de_DE.UTF-8", LC_ALL="de_DE.UTF-8", LANGUAGE="de")
    run("/bin/brush", "--login", "-i", "-c", 'test -f "$XDG_CONFIG_HOME/user-dirs.dirs"')
    assert run(LOOKUP, "DOCUMENTS") == str(german / "Dokumente"), "translated folder name was not used"
    assert (german / "Dokumente").is_dir()
print("XDG user directories: login creation, ownership, lookup, repeat login, custom paths, opt-out and localization passed")
