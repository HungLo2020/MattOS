
project('shared-mime-info',
    'c', 'cpp',
    version: '2.5.1',
    meson_version: '>=0.49.0',
    default_options : ['cpp_std=c++17']
)

config = configuration_data()

i18n = import('i18n')

cc = meson.get_compiler('c')
cxx = meson.get_compiler('cpp')

###############################################################################
# Project configuration

config.set_quoted('PACKAGE', meson.project_name())
config.set_quoted('VERSION', meson.project_version())

###############################################################################
# Find tools

xmllint = find_program('xmllint', required: get_option('build-tests'))
xmlto = find_program('xmlto', required: get_option('build-spec'))

###############################################################################
# Check if GCC needs -lstdc++fs (before 9.1)

if not cxx.links('''
        #include <filesystem>
        int main() {
            return std::filesystem::is_directory(
                std::filesystem::status("/tmp")) ? 0 : 1;
        }
        ''', name: 'std++fs-check')
    add_project_link_arguments('-lstdc++fs', language : 'cpp')
endif

###############################################################################
# Dependencies

config.set('HAVE_FDATASYNC', cc.has_function('fdatasync', prefix: '#include <unistd.h>'))


if get_option('build-translations')
    subdir('po')
endif
subdir('data')
if get_option('build-tools')
    libxml = dependency('libxml-2.0',   version: '>=2.4')
    glib2  = dependency('glib-2.0',     version: '>=2.6.0')

    gio = dependency('gio-2.0', required: false)
    subdir('src')
endif

build_tests = get_option('build-tests')
if meson.is_cross_build()
    # It's currently impossible to use a native subproject when cross-compiling.
    warning('Cannot cross-compile tests due to https://github.com/mesonbuild/meson/issues/11121')
    build_tests = false
endif
if build_tests
    # Find xdgmime. It needs to be a native dependency (i.e. compiled for the
    # build system rather than the host system) so we can run its programs as
    # part of the shared-mime-info build.
    subproject('xdgmime')
    xdgmime_dep = dependency('xdgmime', native: true)

    xdgmime_print_mime_data = find_program('print-mime-data')
    xdgmime_test_mime_data  = find_program('test-mime-data')
    xdgmime_test_mime       = find_program('test-mime')
    subdir('tests')
endif

configure_file(
    input: 'shared-mime-info.pc.in',
    output: '@BASENAME@',
    configuration: {
        'prefix': get_option('prefix'),
        'VERSION': meson.project_version()
    },
    install_dir: get_option('datadir') / 'pkgconfig'
)

if get_option('update-mimedb')
    upd_tool = (meson.is_cross_build() or not get_option('build-tools')
        ? find_program('update-mime-database').path()
        : update_mime_database.full_path()
    )
    meson.add_install_script('sh', '-c', ' '.join([
        upd_tool, '-V', '${MESON_INSTALL_DESTDIR_PREFIX}' / get_option('datadir') / 'mime',
    ]))
endif
