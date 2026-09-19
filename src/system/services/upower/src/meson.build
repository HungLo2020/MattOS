
upowerd_deps = declare_dependency(
    include_directories: [
        include_directories('.'),
        include_directories('..'),
        include_directories('../dbus'),
    ],
    dependencies: [
        m_dep, glib_dep, gobject_dep, gio_dep, gio_unix_dep, libupower_glib_dep, upowerd_dbus_dep, polkit
    ],
    compile_args: [
        '-DUP_COMPILATION',
        '-DHISTORY_DIR="@0@"'.format(historydir),
        '-DSTATE_DIR="@0@"'.format(statedir),
    ],
)

upshared = {}
subdir('dummy')
subdir('linux')
subdir('bsd')
subdir('openbsd')
subdir('freebsd')

# Everything that is also needed by the tests
upowerd_private = static_library('upowerd-private',
    sources: [
        'up-constants.h',
        'up-config.h',
        'up-config.c',
        'up-daemon.h',
        'up-daemon.c',
        'up-device.h',
        'up-device.c',
        'up-device-battery.h',
        'up-device-battery.c',
        'up-device-list.h',
        'up-device-list.c',
        'up-enumerator.c',
        'up-enumerator.h',
        'up-device-kbd-backlight.c',
        'up-device-kbd-backlight.h',
        'up-history.h',
        'up-history.c',
        'up-backend.h',
        'up-native.h',
        'up-common.h',
        'up-common.c',
        'up-polkit.c',
        'up-polkit.h',
    ],
    dependencies: [ upowerd_deps ],
    c_args: [ '-DG_LOG_DOMAIN="UPower"' ],
)

upowerd = executable('upowerd',
    sources: [
        'up-main.c',
    ],
    dependencies: upowerd_deps,
    link_with: [ upowerd_private, upshared[os_backend] ],
    gnu_symbol_visibility: 'hidden',
    install: true,
    install_dir: get_option('prefix') / get_option('libexecdir'),
    c_args: [ '-DG_LOG_DOMAIN="UPower"' ],
)

up_self_test = executable('up_self_test',
    sources: [
        'up-self-test.c',
    ],
    c_args: [
        '-DUPOWER_CONF_PATH="@0@"'.format(meson.project_source_root() / 'etc' / 'UPower.conf'),
        '-DG_LOG_DOMAIN="UPower"',
    ],
    dependencies: upowerd_deps,
    link_with: [ upowerd_private, upshared['dummy'] ],
    gnu_symbol_visibility: 'hidden',
    build_by_default: true,
    install: false,
)

#############
# Data/Config files
#############

install_emptydir(historydir)

cdata = configuration_data()
cdata.set('libexecdir', get_option('prefix') / get_option('libexecdir'))
cdata.set('historydir', historydir)

configure_file(
    input: 'org.freedesktop.UPower.service.in',
    output: 'org.freedesktop.UPower.service',
    install_dir: dbusdir / 'system-services',
    configuration: cdata,
)

configure_file(
    input: 'org.freedesktop.UPower.conf.in',
    output: 'org.freedesktop.UPower.conf',
    install_dir: dbusdir / 'system.d',
    configuration: cdata,
)

if systemdsystemunitdir != 'no'
    configure_file(
        input: 'upower.service.in',
        output: 'upower.service',
        install_dir: systemdsystemunitdir,
        configuration: cdata,
    )
endif

#############
# Tests
#############
test(
   'self-test',
   up_self_test,
)

# On Linux, we can run the additional integration test;
# defined here as we would have a circular dependency otherwise.
if os_backend == 'linux' and gobject_introspection.found()
    env = environment()
    env.prepend('top_builddir', join_paths(meson.project_build_root()))
    env.prepend('LD_LIBRARY_PATH', join_paths(meson.project_build_root(), 'libupower-glib'))
    env.prepend('GI_TYPELIB_PATH', join_paths(meson.project_build_root(), 'libupower-glib'))

    python3 = find_program('python3')
    unittest_inspector = find_program('linux/unittest_inspector.py')
    r = run_command(unittest_inspector, files('linux/integration-test.py'), check: false)
    unit_tests = r.stdout().strip().split('\n')

    foreach ut: unit_tests
        ut_args = files('linux/integration-test.py')
        ut_args += ut
        test(ut,
             python3,
             args: ut_args,
             env: env,
             depends: [ libupower_glib_typelib, upowerd ],
             timeout: 150,
            )
    endforeach

    installed_tests = get_option('installed_tests')
    if installed_tests
        install_data( [
            'linux/integration-test.py',
            'linux/output_checker.py',
          ],
          install_dir: get_option('prefix') / get_option('libexecdir') / 'upower'
        )
        install_subdir('linux/tests/',
          install_dir: get_option('prefix') / get_option('libexecdir') / 'upower'
        )

        configure_file(
          input: 'upower-integration.test.in',
          output: 'upower-integration.test',
          install_dir: get_option('datadir') / 'installed-tests' / 'upower',
          configuration: cdata
        )
    endif
endif
