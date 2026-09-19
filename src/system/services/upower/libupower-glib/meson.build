version_arr = meson.project_version().split('.')
major_version = version_arr[0].to_int()
minor_version = version_arr[1].to_int()
micro_version = version_arr[2].to_int()

cdata = configuration_data()
cdata.set('UP_MAJOR_VERSION', major_version)
cdata.set('UP_MINOR_VERSION', minor_version)
cdata.set('UP_MICRO_VERSION', micro_version)

up_version_h = configure_file(
    output: 'up-version.h',
    input: 'up-version.h.in',
    configuration: cdata,
)

libupower_glib_headers = [
    'upower.h',
    'up-autocleanups.h',
    'up-types.h',
    'up-device.h',
    'up-stats-item.h',
    'up-history-item.h',
    'up-client.h',
    up_version_h,
]

libupower_glib_sources = [
    'up-types.c',
    'up-client.c',
    'up-stats-item.c',
    'up-history-item.c',
    'up-device.c',
]

install_headers(libupower_glib_headers,
    subdir: 'libupower-glib'
)

libupower_glib = shared_library('upower-glib',
    sources: libupower_glib_headers + libupower_glib_sources,
    dependencies: [ gobject_dep, gio_dep, upowerd_dbus_dep ],
    include_directories: [ '..' ],
    c_args: [
        '-DUP_COMPILATION',
        '-DG_LOG_DOMAIN="libupower-glib"',
    ],
    soversion: soversion,
    version: libversion,
    install: true,
)

libupower_glib_dep = declare_dependency(
    sources: libupower_glib_headers,
    link_with: libupower_glib,
    # FIXME: The libupower-glib prefix is used inconsistently when including
    include_directories: [ '.', '..' ],
    dependencies: [ gobject_dep, gio_dep ],
)

if gobject_introspection.found()
    gir = gnome.generate_gir(libupower_glib,
        sources : libupower_glib_headers + libupower_glib_sources,
        namespace : 'UPowerGlib',
        symbol_prefix : 'up_',
        identifier_prefix : 'Up',
        extra_args : [
            '--c-include=upower.h',
        ],
        link_with : libupower_glib,
        nsversion: '1.0',
        dependencies : [
            gobject_dep,
            gio_dep,
            declare_dependency(compile_args: [ '-DUP_COMPILATION' ]),
        ],
        includes : [
            'GObject-2.0',
            'Gio-2.0',
        ],
        install : true)

    libupower_glib_typelib = gir[1]
endif
