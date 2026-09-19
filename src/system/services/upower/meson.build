project('upower', 'c',
    version: '1.91.4',
    license: 'GPLv2+',
    default_options: [
        'buildtype=debugoptimized',
        'warning_level=1',
        'c_std=gnu99',
    ],
    meson_version: '>= 0.60.0')

soversion = 3
current = 1
revision = 2
libversion = '@0@.@1@.@2@'.format(soversion, current, revision)

gnome = import('gnome')
i18n = import('i18n')

cc = meson.get_compiler('c')

# TODO: Get rid of these by including config.h where needed
add_project_arguments([
    '-DGETTEXT_PACKAGE="@0@"'.format(meson.project_name()),
    '-DPACKAGE_VERSION="@0@"'.format(meson.project_version()),
], language: 'c')

cdata = configuration_data()
cdata.set_quoted('GETTEXT_PACKAGE', meson.project_name())
cdata.set_quoted('PACKAGE_VERSION', meson.project_version())
cdata.set_quoted('VERSION', meson.project_version())
cdata.set_quoted('PACKAGE_SYSCONF_DIR', get_option('sysconfdir'))

glib_min_version = '2.76'

glib_version_def = 'GLIB_VERSION_@0@_@1@'.format(
    glib_min_version.split('.')[0], glib_min_version.split('.')[1])
common_cflags = cc.get_supported_arguments([
    '-DGLIB_VERSION_MIN_REQUIRED=' + glib_version_def,
    '-DGLIB_VERSION_MAX_ALLOWED=' + glib_version_def,
])
add_project_arguments(common_cflags, language: 'c')


glib_dep = dependency('glib-2.0', version: '>=' + glib_min_version)
gobject_dep = dependency('gobject-2.0', version: '>=' + glib_min_version)
gio_dep = dependency('gio-2.0', version: '>=' + glib_min_version)
gio_unix_dep = dependency('gio-unix-2.0', version: '>=' + glib_min_version)
m_dep = cc.find_library('m', required: true)

polkit = dependency('polkit-gobject-1', version: '>= 0.103',
                    required: get_option('polkit').disable_auto_if(host_machine.system() != 'linux'))
if polkit.found()
  cdata.set('HAVE_POLKIT', '1')
  if polkit.version().version_compare('>= 0.114')
    cdata.set('HAVE_POLKIT_0_114', '1')
  endif
  cdata.set_quoted ('POLKIT_ACTIONDIR', polkit.get_variable(pkgconfig: 'actiondir'))
endif

xsltproc = find_program('xsltproc', disabler: true, required: get_option('gtk-doc') or get_option('man'))

# Resolve OS backend
os_backend = get_option('os_backend')
if os_backend == 'auto'
  # Likely needs to be updated when options are added
  if host_machine.system() in ['linux', 'freebsd', 'openbsd']
    os_backend = host_machine.system()
  else
    os_backend = 'dummy'
  endif
endif
cdata.set_quoted('BACKEND', os_backend)

# Backend specific dependencies
gudev_dep = dependency('', required: false)
idevice_dep = dependency('', required: false)
plist_dep = dependency('', required: false)
gobject_introspection = dependency('gobject-introspection-1.0', required: get_option('introspection'))


if os_backend == 'linux'
  gudev_dep = dependency('gudev-1.0', version: '>= 238')
  idevice_dep = dependency('libimobiledevice-1.0',
                           version : '>= 0.9.7',
                           required : get_option('idevice'))

  if idevice_dep.found()
    plist_dep = dependency('libplist-2.0', required: false)
    if not plist_dep.found()
      plist_dep = dependency('libplist', required: true)
    endif
    cdata.set10('HAVE_IDEVICE', true)
  endif

  udevrulesdir = get_option('udevrulesdir')
  if udevrulesdir == 'auto'
    udev_dep = dependency('udev', required: true)
    udevrulesdir = udev_dep.get_variable(pkgconfig: 'udev_dir') / 'rules.d'
  endif

  udevhwdbdir = get_option('udevhwdbdir')
  if udevhwdbdir == 'auto'
    udev_dep = dependency('udev', required: true)
    udevhwdbdir = udev_dep.get_variable(pkgconfig: 'udev_dir') / 'hwdb.d'
  endif
endif

historydir = get_option('historydir')
if historydir == ''
    historydir = get_option('prefix') / get_option('localstatedir') / 'lib' / 'upower'
endif

statedir = get_option('statedir')
if statedir == ''
    statedir = get_option('prefix') / get_option('localstatedir') / 'lib' / 'upower'
endif

dbusdir = get_option('datadir') / 'dbus-1'
systemdsystemunitdir = get_option('systemdsystemunitdir')
if systemdsystemunitdir == ''
    systemd_dep = dependency('systemd')
    systemdsystemunitdir = systemd_dep.get_variable(pkgconfig: 'systemdsystemunitdir')
endif

datadir = get_option('datadir')
if datadir == ''
    datadir = join_paths(prefix, get_option('datadir'))
endif

# Generate configuration file
config_h = configure_file(output: 'config.h', configuration: cdata)

subdir('etc')
subdir('rules')
subdir('po')
subdir('dbus')
subdir('libupower-glib')
subdir('src')
subdir('tools')
subdir('doc')
subdir('data/zsh-completion')
if polkit.found()
    subdir('policy')
endif

pkgconfig = import('pkgconfig')
pkgconfig.generate(
    name: 'upower-glib',
    description: 'UPower is a system daemon for managing power devices',
    version: meson.project_version(),
    libraries: libupower_glib,
    requires: [glib_dep, gobject_dep, polkit],
    subdirs: 'libupower-glib',
)

output = []
output += 'UPower ' + meson.project_version()
output += 'System Paths'
output += '  prefix:         ' + get_option('prefix')
output += '  libdir:         ' + get_option('libdir')
output += '  libexecdir:     ' + get_option('prefix') / get_option('libexecdir')
output += '  bindir:         ' + get_option('prefix') / get_option('bindir')
output += '  sbindir:        ' + get_option('prefix') / get_option('sbindir')
output += '  datadir:        ' + get_option('prefix') / get_option('datadir')
output += '  sysconfdir:     ' + get_option('sysconfdir')
output += '  localstatedir:  ' + get_option('prefix') / get_option('localstatedir')
output += '  historydir:     ' + historydir

output += '\nFeatures'
output += '  Backend:                  ' + os_backend
output += '  libimobiledevice support: ' + idevice_dep.found().to_string()
output += '  Building api docs:        ' + get_option('gtk-doc').to_string()
output += '  Building man pages:       ' + get_option('man').to_string()

message('\n'+'\n'.join(output)+'\n')
