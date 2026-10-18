#!/usr/bin/env python3
"""Build optional Kate and KWrite against the MattOS Qt/KDE development SDK."""

from __future__ import annotations

import os
import re
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from common import RecipeError, SourceReleaseRecipe, command, install_license, run_recipe
from common.build import cmake_build_install, download, extract_archive


class KateRecipe(SourceReleaseRecipe):
    name = "kate"
    repository = "mattos"
    section = "editors"
    description = "Optional KDE Kate and KWrite text editors"
    # Platform and SVG image plugins are loaded dynamically, so ldd cannot
    # derive these runtime dependencies from the editor's ELF files.
    depends = ("kf6-ktexteditor", "breeze-icons", "qt6-wayland", "qt6-svg")
    provides = ("kwrite",)
    git_url = "https://invent.kde.org/utilities/kate.git"
    tag_pattern = r"v([0-9]+\.[0-9]+\.[0-9]+)"
    source_url = "https://download.kde.org/stable/release-service/{version}/src/kate-{version}.tar.xz"

    def build_source(self, source: Path, workspace: Path, staging: Path) -> None:
        # KF6I18n's public CMake macros require real translation compilers.
        # Keep these build-only tools inside the disposable workspace.
        gettext_archive = workspace / "gettext-1.0.tar.xz"
        download("https://ftp.gnu.org/gnu/gettext/gettext-1.0.tar.xz", gettext_archive,
                 sha256="71132a3fb71e68245b8f2ac4e9e97137d3e5c02f415636eb508ae607bc01add7")
        gettext = extract_archive(gettext_archive, workspace / "gettext-source")
        tools = workspace / "gettext-install"
        gettext_build = workspace / "gettext-build"
        gettext_build.mkdir()
        build_env = {**os.environ, "MAKEINFO": "true"}
        command([str(gettext / "configure"), f"--prefix={tools}", "--disable-shared",
                 "--disable-nls", "--disable-java", "--disable-csharp", "--disable-libasprintf",
                 "--disable-maintainer-mode", "--without-emacs"], cwd=gettext_build, env=build_env)
        command(["make", "-j", "4"], cwd=gettext_build, env=build_env)
        command(["make", "install"], cwd=gettext_build, env=build_env)
        os.environ["PATH"] = str(tools / "bin") + os.pathsep + os.environ["PATH"]
        for program in ("msgfmt", "msgmerge"):
            command([str(tools / "bin" / program), "--version"])
        # ECM is a checksum-pinned, build-only CMake dependency. It is never
        # copied from the host or installed in the editor package.
        archive = workspace / "extra-cmake-modules-6.27.0.tar.xz"
        download(
            "https://download.kde.org/stable/frameworks/6.27/extra-cmake-modules-6.27.0.tar.xz",
            archive,
            sha256="f3b5af578017a6a0f127fb1c31609db2e4f016e99b900d5a11f6ffb6c55006a3",
        )
        ecm = extract_archive(archive, workspace / "ecm-source")
        prefix = workspace / "ecm-install"
        cmake_build_install(ecm, workspace / "ecm-build", prefix,
                            options=("-DBUILD_TESTING=OFF", "-DBUILD_DOC=OFF"))
        # Qt's cross install ships the native generators but leaves their
        # QmlTools CMake export in its build tree. Describe the real SDK tools
        # in this consumer's workspace, without importing a host Qt or changing
        # the installed target packages.
        qt_version = command(["qmake6", "-query", "QT_VERSION"]).strip()
        if not re.fullmatch(r"[0-9]+\.[0-9]+\.[0-9]+", qt_version):
            raise RecipeError(f"Invalid SDK Qt version: {qt_version!r}")
        qml_tools = workspace / "qml-tools"
        qml_tools.mkdir()
        config = ["set(Qt6QmlTools_FOUND TRUE)"]
        for directory, names in (
            ("bin", ("qmllint", "qmlcontextpropertydump", "qmlformat", "qmltc")),
            ("libexec", ("qmltyperegistrar", "qmlcachegen", "qmlaotstats", "qmlimportscanner")),
        ):
            for name in names:
                executable = Path("/usr") / directory / name
                if not executable.is_file() or not os.access(executable, os.X_OK):
                    raise RecipeError(f"SDK QML generator missing: {executable}")
                config.extend((f"if(NOT TARGET Qt6::{name})",
                               f"  add_executable(Qt6::{name} IMPORTED GLOBAL)",
                               f'  set_target_properties(Qt6::{name} PROPERTIES IMPORTED_LOCATION "{executable}")',
                               "endif()"))
        (qml_tools / "Qt6QmlToolsConfig.cmake").write_text("\n".join(config) + "\n")
        (qml_tools / "Qt6QmlToolsConfigVersion.cmake").write_text(
            f'set(PACKAGE_VERSION "{qt_version}")\n'
            'if(PACKAGE_FIND_VERSION VERSION_EQUAL PACKAGE_VERSION)\n'
            '  set(PACKAGE_VERSION_COMPATIBLE TRUE)\n'
            '  set(PACKAGE_VERSION_EXACT TRUE)\n'
            'endif()\n')
        cmake_build_install(
            source, workspace / "build", staging,
            options=("-GNinja", "-DCMAKE_JOB_POOLS=mattos_compile=4;mattos_link=1",
                     "-DCMAKE_JOB_POOL_COMPILE=mattos_compile", "-DCMAKE_JOB_POOL_LINK=mattos_link",
                     "-DCMAKE_AUTOGEN_PARALLEL=4",
                     f"-DECM_DIR={prefix}/usr/share/ECM/cmake", "-DBUILD_TESTING=OFF",
                     f"-DQt6QmlTools_DIR={qml_tools}",
                     "-DBUILD_DOC=OFF", "-DBUILD_doc=OFF", "-DENABLE_PCH=OFF",
                     # Shared Qt loads plugins at runtime. Avoid importing
                     # CMake targets for intentionally unshipped QML tests.
                     "-DQT_SKIP_AUTO_QML_PLUGIN_INCLUSION=ON",
                     "-DKDE_INSTALL_USE_QT_SYS_PATHS=OFF"),
        )

    def post_install(self, staging: Path, source: Path) -> None:
        for binary in ("kate", "kwrite"):
            if not (staging / "usr/bin" / binary).is_file():
                raise RecipeError(f"Kate build did not install {binary}")
        install_license(staging, self.name, source, ["LICENSES/GPL-2.0-or-later.txt"])


if __name__ == "__main__":
    try:
        raise SystemExit(run_recipe(KateRecipe(), sys.argv[1:], Path(__file__).resolve()))
    except RecipeError as exc:
        print(f"error: {exc}", file=sys.stderr)
        raise SystemExit(1)
