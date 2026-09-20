// Source-owned Plasma desktop services and core applications. Keep these
// products separate in the graph while sharing the strict KDE CMake builder.

fn build_plasma_app(repo_root: &Path, stage: &str, components: &[&str], options: &[&str], output: &str) -> Result<()> {
    build_kde_cmake(repo_root, stage, &format!("src/desktop/kde/{stage}"), components, options, output)
}

fn build_kfilemetadata(r: &Path) -> Result<()> { build_plasma_app(r, "kfilemetadata", &["qtbase","karchive","kcodecs","kconfig","kcoreaddons","ki18n","attr"], &["-DBUILD_TESTING=OFF","-DBUILD_QCH=OFF"], "usr/lib/x86_64-linux-gnu/libKF6FileMetaData.so") }
fn build_kpty(r: &Path) -> Result<()> { build_plasma_app(r, "kpty", &["qtbase","kcoreaddons","ki18n","util-linux"], &["-DBUILD_TESTING=OFF","-DBUILD_QCH=OFF"], "usr/lib/x86_64-linux-gnu/libKF6Pty.so") }
fn build_networkmanager_qt(r: &Path) -> Result<()> { build_plasma_app(r, "networkmanager-qt", &["qtbase","qtdeclarative","networkmanager","glib","dbus","libffi","pcre2","zlib"], &["-DBUILD_TESTING=OFF","-DBUILD_QCH=OFF"], "usr/lib/x86_64-linux-gnu/libKF6NetworkManagerQt.so") }
fn build_modemmanager(r: &Path) -> Result<()> {
    let tool_dir = r.join("out/host-tools/modemmanager-xslt");
    fs::create_dir_all(&tool_dir)?;
    let xsltproc = tool_dir.join("xsltproc");
    fs::write(&xsltproc, r#"#!/usr/bin/python3
from pathlib import Path
import sys
import xml.etree.ElementTree as etree

args = [arg for arg in sys.argv[1:] if arg not in ("--xinclude", "--nonet")]
if len(args) != 4 or args[0] != "--output":
    raise SystemExit("MattOS xsltproc shim expects: --xinclude --nonet --output OUTPUT STYLESHEET XML")
if Path(args[2]).name != "header-generator.xsl":
    raise SystemExit("MattOS ModemManager generator received an unsupported stylesheet")

xml_path = Path(args[3])
root = etree.parse(xml_path).getroot()
xi = "{http://www.w3.org/2001/XInclude}include"
interfaces = []
for include in root.findall(xi):
    included = etree.parse(xml_path.parent / include.attrib["href"]).getroot()
    interfaces.extend(included.findall("interface"))

def macro_name(value):
    return value.upper().replace(".", "_").replace(" ", "_")

lines = ["""/* Generated Header file do not edit */

#ifndef _MODEM_MANAGER_NAMES_H_
#define _MODEM_MANAGER_NAMES_H_

#define MM_DBUS_PATH    \"/org/freedesktop/ModemManager1\"
#define MM_DBUS_SERVICE \"org.freedesktop.ModemManager1\"
#define MM_DBUS_MODEM_PREFIX  MM_DBUS_PATH \"/Modem\"
#define MM_DBUS_BEARER_PREFIX MM_DBUS_PATH \"/Bearer\"
#define MM_DBUS_CBM_PREFIX    MM_DBUS_PATH \"/CBM\"
#define MM_DBUS_SIM_PREFIX    MM_DBUS_PATH \"/SIM\"
#define MM_DBUS_SMS_PREFIX    MM_DBUS_PATH \"/SMS\"
#define MM_DBUS_CALL_PREFIX   MM_DBUS_PATH \"/Call\"
#define MM_DBUS_ERROR_PREFIX \"org.freedesktop.ModemManager1.Error\"
"""]
prefix = "org.freedesktop.ModemManager1"
for interface in interfaces:
    name = interface.attrib["name"]
    suffix = name[len(prefix):].lstrip(".") if name.startswith(prefix) else ""
    macro = "MM_DBUS_INTERFACE" + ("_" + macro_name(suffix) if suffix else "")
    lines.append(f'#define {macro} "{name}"')

for interface in interfaces:
    name = interface.attrib["name"]
    suffix = name[len(prefix):].lstrip(".") if name.startswith(prefix + ".") else "MANAGER"
    interface_macro = macro_name(suffix)
    lines.append(f'\n/* Interface \'{name}\' */')
    for tag, kind in (("method", "METHOD"), ("signal", "SIGNAL"), ("property", "PROPERTY")):
        for member in interface.findall(tag):
            member_name = member.attrib["name"]
            lines.append(f'#define MM_{interface_macro}_{kind}_{macro_name(member_name)} "{member_name}"')

lines.append("\n#endif /* _MODEM_MANAGER_NAMES_H_ */\n")
Path(args[1]).write_text("\n".join(lines), encoding="ascii")
"#)?;
    fs::set_permissions(&xsltproc, <fs::Permissions as std::os::unix::fs::PermissionsExt>::from_mode(0o755))?;
    let path = format!("{}:{}", tool_dir.display(), std::env::var("PATH").unwrap_or_default());
    build_meson_runtime(r, "modemmanager", "src/system/services/modemmanager", &["glib","dbus","pcre2","zlib","libffi"], &[
        "--prefix=/usr", "--libdir=lib/x86_64-linux-gnu",
        "-Dudev=false", "-Dudevdir=/usr/lib/udev", "-Dexamples=false", "-Dtests=false", "-Dsystemdsystemunitdir=no", "-Dsystemd_suspend_resume=false",
        "-Dsystemd_journal=false", "-Dpolkit=no", "-Dmbim=false", "-Dqmi=false", "-Dqrtr=false",
        "-Dintrospection=false", "-Dvapi=false", "-Dman=false", "-Dgtk_doc=false", "-Dbash_completion=false",
    ], "usr/lib/x86_64-linux-gnu/libmm-glib.so", &[("PATH", path)])?;
    // ModemManagerQt includes the public API as <ModemManager/ModemManager.h>.
    // ModemManager 1.24's metadata adds that directory a second time; publish
    // a target-owned compatibility descriptor in the disposable output only.
    let pc = r.join("out/build/modemmanager/install/usr/lib/x86_64-linux-gnu/pkgconfig/ModemManager.pc");
    let body = fs::read_to_string(&pc)?;
    let fixed = body.replace("Cflags: -I${includedir}/ModemManager", "Cflags: -I${includedir} -I${includedir}/ModemManager");
    if fixed == body { bail!("ModemManager.pc no longer contains the expected include contract"); }
    fs::write(pc, fixed)?;
    Ok(())
}
fn build_modemmanager_qt(r: &Path) -> Result<()> {
    build_plasma_app(r, "modemmanager-qt", &["qtbase","qtdeclarative","modemmanager","glib","dbus","pcre2","zlib","libffi"], &["-DBUILD_TESTING=OFF","-DBUILD_QCH=OFF"], "usr/lib/x86_64-linux-gnu/libKF6ModemManagerQt.so")?;

    // Upstream exports the absolute include paths reported by ModemManager's
    // pkg-config file.  They are correct after package installation, but are
    // neither relocatable nor usable by the isolated staged-prefix builds.
    // Make the installed target relative to its prefix and hydrate the
    // dependency headers into this disposable SDK output.  Package staging
    // keeps those headers owned exclusively by ModemManager.
    let install = r.join("out/build/modemmanager-qt/install/usr");
    let targets = install.join("lib/x86_64-linux-gnu/cmake/KF6ModemManagerQt/KF6ModemManagerQtTargets.cmake");
    let contents = fs::read_to_string(&targets)?;
    let adjusted = contents.replace(
        ";/usr/include;/usr/include/ModemManager\"",
        ";${_IMPORT_PREFIX}/include;${_IMPORT_PREFIX}/include/ModemManager\"",
    );
    if adjusted == contents {
        bail!("ModemManagerQt export no longer contains the expected absolute include paths");
    }
    fs::write(&targets, adjusted)?;
    let dependency_headers = r.join("out/build/modemmanager/install/usr/include/ModemManager");
    let staged_headers = install.join("include/ModemManager");
    fs::create_dir_all(&staged_headers)?;
    let source_arg = format!("{}/", dependency_headers.display());
    let destination_arg = format!("{}/", staged_headers.display());
    run_cmd(r, "rsync", &["-a", &source_arg, &destination_arg])?;
    Ok(())
}
fn build_purpose(r: &Path) -> Result<()> { build_plasma_app(r, "purpose", &["qtbase","qtdeclarative","karchive","kauth","kbookmarks","kcolorscheme","kcompletion","kconfig","kcoreaddons","kcrash","kdbusaddons","kguiaddons","ki18n","kiconthemes","kitemmodels","kitemviews","kjobwidgets","knotifications","kio","kservice","kwidgetsaddons","kwindowsystem","solid","util-linux","kirigami","prison","kcmutils"], &["-DBUILD_TESTING=OFF","-DBUILD_QCH=OFF"], "usr/lib/x86_64-linux-gnu/libKF6Purpose.so") }
fn build_milou(r: &Path) -> Result<()> { build_plasma_app(r, "milou", &["qtbase","qtdeclarative","kconfig","kcoreaddons","ki18n","krunner","ksvg","kpackage","kirigami","kwindowsystem","plasma-framework"], &["-DBUILD_TESTING=OFF"], "usr/lib/x86_64-linux-gnu/qml/org/kde/milou/libmilou.so") }
fn build_systemsettings(r: &Path) -> Result<()> { build_plasma_app(r, "systemsettings", &["qtbase","qtdeclarative","karchive","kauth","kbookmarks","kcodecs","kcolorscheme","kcompletion","kconfig","kconfigwidgets","kcoreaddons","kcrash","kdbusaddons","kglobalaccel","kguiaddons","ki18n","kiconthemes","breeze-icons","kitemmodels","kitemviews","kjobwidgets","knotifications","kcmutils","kio","kservice","ktextwidgets","kwidgetsaddons","kwindowsystem","kxmlgui","solid","util-linux","libffi","kirigami","krunner","plasma-activities"], &["-DBUILD_TESTING=OFF","-DBUILD_DOC=OFF"], "usr/bin/systemsettings") }
fn build_ksystemstats(r: &Path) -> Result<()> { build_plasma_app(r, "ksystemstats", &["qtbase","qtdeclarative","kbookmarks","kcompletion","kconfig","kcoreaddons","kcrash","ki18n","kitemviews","kjobwidgets","kservice","kwidgetsaddons","kwindowsystem","kio","ksysguard","solid","networkmanager-qt","networkmanager","glib","dbus","pcre2","zlib","systemd","libnl","libffi","lm-sensors","libdrm"], &["-DBUILD_TESTING=OFF"], "usr/bin/ksystemstats") }
fn build_plasma_systemmonitor(r: &Path) -> Result<()> { build_plasma_app(r, "plasma-systemmonitor", &["qtbase","qtdeclarative","karchive","kauth","kbookmarks","kcodecs","kcolorscheme","kcompletion","kconfig","kconfigwidgets","kcoreaddons","kcrash","kdbusaddons","kglobalaccel","kguiaddons","ki18n","kiconthemes","kitemmodels","kitemviews","kjobwidgets","knotifications","kservice","ktextwidgets","kwidgetsaddons","kwindowsystem","kio","solid","util-linux","knewstuff","attica","kpackage","ksysguard","ksystemstats","kirigami","kirigami-addons","breeze-icons","libffi"], &["-DBUILD_TESTING=OFF","-DBUILD_DOC=OFF"], "usr/bin/plasma-systemmonitor") }
fn build_polkit_kde_agent(r: &Path) -> Result<()> { build_plasma_app(r, "polkit-kde-agent-1", &["qtbase","qtdeclarative","kconfig","ki18n","kwindowsystem","knotifications","kdbusaddons","kcoreaddons","kcrash","libcanberra","polkit-qt-1","polkit","glib","dbus","zlib","libffi"], &["-DBUILD_TESTING=OFF"], "usr/lib/x86_64-linux-gnu/libexec/polkit-kde-authentication-agent-1") }
fn build_kquickimageeditor(r: &Path) -> Result<()> { build_plasma_app(r, "kquickimageeditor", &["qtbase","qtdeclarative","qtshadertools","kconfig","kirigami","highway"], &["-DBUILD_TESTING=OFF"], "usr/lib/x86_64-linux-gnu/qml/org/kde/kquickimageeditor/libKQuickImageEditorplugin.so") }
fn build_kpipewire(r: &Path) -> Result<()> { build_plasma_app(r, "kpipewire", &["qtbase","qtdeclarative","kcoreaddons","ki18n","pipewire","wayland","plasma-wayland-protocols","libglvnd","mesa","libepoxy","libdrm","ffmpeg","libva"], &["-DBUILD_TESTING=OFF","-DBUILD_EXAMPLES=OFF"], "usr/lib/x86_64-linux-gnu/libKPipeWire.so") }
fn build_spectacle(r: &Path) -> Result<()> { build_plasma_app(r, "spectacle", &["qtbase","qtdeclarative","qtshadertools","qtmultimedia","karchive","kauth","kbookmarks","kcodecs","kcolorscheme","kcompletion","kconfig","kconfigwidgets","kcoreaddons","kcrash","kdbusaddons","kglobalaccel","kguiaddons","ki18n","kiconthemes","breeze-icons","kitemmodels","kitemviews","kjobwidgets","knotifications","kio","kservice","kstatusnotifieritem","ktextwidgets","kwidgetsaddons","kwindowsystem","kxmlgui","solid","util-linux","kirigami","prison","zxing-cpp","layer-shell-qt","kpipewire","pipewire","ffmpeg","libva","mesa","libepoxy","libdrm","libglvnd","libcanberra","kquickimageeditor","highway","purpose","opencv","plasma-wayland-protocols","wayland","xkbcommon","libffi"], &["-DBUILD_TESTING=OFF","-DBUILD_DOC=OFF"], "usr/bin/spectacle") }
fn build_pulseaudio_qt(r: &Path) -> Result<()> { build_plasma_app(r, "pulseaudio-qt", &["qtbase","glib","pulseaudio"], &["-DBUILD_TESTING=OFF"], "usr/lib/x86_64-linux-gnu/libKF6PulseAudioQt.so") }
fn build_plasma_pa(r: &Path) -> Result<()> { build_plasma_app(r, "plasma-pa", &["qtbase","qtdeclarative","kconfig","kconfigwidgets","kcodecs","kcolorscheme","kwidgetsaddons","kcoreaddons","kdbusaddons","kdeclarative","kglobalaccel","ki18n","kstatusnotifieritem","kcmutils","ksvg","kpackage","kirigami","kirigami-addons","kitemmodels","kwindowsystem","plasma-framework","pulseaudio-qt","pulseaudio","glib","libcanberra"], &["-DBUILD_TESTING=OFF","-DBUILD_DOC=OFF"], "usr/lib/x86_64-linux-gnu/qml/org/kde/plasma/private/volume/libplasma-volume-declarative.so") }
fn build_plasma_nm(r: &Path) -> Result<()> { build_plasma_app(r, "plasma-nm", &["qtbase","qtdeclarative","qca","qcoro","kcmutils","kconfig","kconfigwidgets","kcoreaddons","kdbusaddons","ki18n","kio","kservice","kwallet","kwidgetsaddons","kwindowsystem","kirigami","networkmanager-qt","modemmanager","modemmanager-qt","plasma-framework","networkmanager","kcolorscheme","kcompletion","kjobwidgets","knotifications","solid","ksvg","prison","kquickcharts","kpackage","karchive","kauth","kbookmarks","kcodecs","kcrash","kguiaddons","kiconthemes","kitemmodels","kitemviews","ktextwidgets","kxmlgui","util-linux","libffi","glib","dbus","pcre2","zlib","systemd","libnl"], &["-DBUILD_TESTING=OFF","-DWITH_MODEMMANAGER_SUPPORT=OFF"], "usr/lib/x86_64-linux-gnu/qml/org/kde/plasma/networkmanagement/libplasmanm_internalplugin.so") }
fn build_powerdevil(r: &Path) -> Result<()> { build_plasma_app(r, "powerdevil", &["qtbase","qtsvg","qtdeclarative","qtwayland","qcoro","kauth","karchive","kbookmarks","kcodecs","kcolorscheme","kcompletion","kconfig","kconfigwidgets","kcoreaddons","kcrash","kdbusaddons","kglobalaccel","kguiaddons","ki18n","kiconthemes","kidletime","kitemmodels","kitemviews","kjobwidgets","kio","kirigami","kcmutils","knotifications","kpackage","krunner","kservice","solid","ksvg","ktextwidgets","kwidgetsaddons","kwindowsystem","kxmlgui","breeze-icons","plasma-framework","plasma-activities","plasma-workspace","libkscreen","polkit-qt-1","libcanberra","libffi","systemd","wayland","plasma-wayland-protocols","x11-compat"], &["-DBUILD_TESTING=OFF","-DWITH_X11=OFF"], "usr/lib/x86_64-linux-gnu/libexec/org_kde_powerdevil") }
fn build_xdg_desktop_portal_kde(r: &Path) -> Result<()> { build_plasma_app(r, "xdg-desktop-portal-kde", &["qtbase","qtsvg","qtdeclarative","karchive","kauth","kbookmarks","kcodecs","kcolorscheme","kcompletion","kconfig","kconfigwidgets","kcoreaddons","kcrash","kdbusaddons","kglobalaccel","kguiaddons","ki18n","kiconthemes","breeze-icons","kitemmodels","kitemviews","kjobwidgets","kio","kirigami","knotifications","kservice","solid","kstatusnotifieritem","ktextwidgets","kwidgetsaddons","kwindowsystem","kxmlgui","kwayland","libkscreen","pipewire","wayland","wayland-protocols","plasma-wayland-protocols","xkbcommon","libcanberra","libffi","systemd","util-linux"], &["-DBUILD_TESTING=OFF"], "usr/lib/x86_64-linux-gnu/libexec/xdg-desktop-portal-kde") }
fn build_dolphin(r: &Path) -> Result<()> { build_plasma_app(r, "dolphin", &["qtbase","qtdeclarative","karchive","kauth","kbookmarks","kcodecs","kcolorscheme","kcompletion","kconfig","kconfigwidgets","kcoreaddons","kcrash","kdbusaddons","kglobalaccel","kguiaddons","ki18n","kiconthemes","breeze-icons","kitemmodels","kitemviews","kjobwidgets","knotifications","kcmutils","kio","knewstuff","attica","kpackage","kparts","kservice","ktextwidgets","kwidgetsaddons","kwindowsystem","kxmlgui","solid","sonnet","util-linux","kirigami","kfilemetadata","xkbcommon","wayland","libcanberra","libffi"], &["-DBUILD_TESTING=OFF","-DBUILD_DOC=OFF"], "usr/bin/dolphin") }
fn build_konsole(r: &Path) -> Result<()> { build_plasma_app(r, "konsole", &["qtbase","qtdeclarative","qtmultimedia","karchive","kauth","kbookmarks","kcodecs","kcolorscheme","kcompletion","kconfig","kconfigwidgets","kcoreaddons","kcrash","kdbusaddons","kglobalaccel","kguiaddons","ki18n","kiconthemes","breeze-icons","kitemmodels","kitemviews","kjobwidgets","kio","knewstuff","attica","knotifications","knotifyconfig","kpackage","kparts","kservice","solid","sonnet","ktextwidgets","kwidgetsaddons","kwindowsystem","kxmlgui","kpty","util-linux","xkbcommon","wayland","libcanberra","libffi","icu"], &["-DBUILD_TESTING=OFF","-DBUILD_DOC=OFF","-DWITH_X11=OFF","-DWITH_LIBSSH=OFF","-DENABLE_PLUGIN_SSHMANAGER=OFF"], "usr/bin/konsole") }
fn build_kate(r: &Path) -> Result<()> { build_plasma_app(r, "kate", &["qtbase","qtdeclarative","qtmultimedia","qtspeech","karchive","kauth","kbookmarks","kcodecs","kcolorscheme","kcompletion","kconfig","kconfigwidgets","kcoreaddons","kcrash","kdbusaddons","kglobalaccel","kguiaddons","ki18n","kiconthemes","breeze-icons","kitemviews","kjobwidgets","kio","knotifications","kparts","kservice","solid","sonnet","ksyntaxhighlighting","ktexteditor","ktextwidgets","kwidgetsaddons","kwindowsystem","kxmlgui","util-linux","xkbcommon","wayland","libcanberra","libffi"], &["-DBUILD_TESTING=OFF","-DBUILD_DOC=OFF"], "usr/bin/kate") }
fn build_ark(r: &Path) -> Result<()> { build_plasma_app(r, "ark", &["qtbase","qtdeclarative","karchive","kauth","kbookmarks","kcodecs","kcolorscheme","kcompletion","kconfig","kconfigwidgets","kcoreaddons","kcrash","kdbusaddons","kglobalaccel","kguiaddons","ki18n","kiconthemes","breeze-icons","kitemmodels","kitemviews","kjobwidgets","knotifications","kfilemetadata","kio","kservice","kparts","solid","kwidgetsaddons","kwindowsystem","kxmlgui","kpty","util-linux","xkbcommon","wayland","libcanberra","libffi","libarchive"], &["-DBUILD_TESTING=OFF","-DBUILD_DOC=OFF"], "usr/bin/ark") }
