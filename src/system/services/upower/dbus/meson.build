
upowerd_dbus_interfaces = [
    [ 'daemon', 'org.freedesktop.UPower', 'Daemon' ],
    [ 'device', 'org.freedesktop.UPower.Device', 'Device' ],
    [ 'kbd-backlight', 'org.freedesktop.UPower.KbdBacklight', 'KbdBacklight' ],
]

upowerd_dbus_headers = []
upowerd_dbus_sources = []
foreach interface: upowerd_dbus_interfaces
    xml = interface[1] + '.xml'
    t = gnome.gdbus_codegen('up-' + interface[0] + '-generated',
        sources: xml,
        autocleanup: 'all',
        annotations:[ [ interface[1], 'org.gtk.GDBus.C.Name', 'Exported' + interface[2] ] ],
        namespace: 'Up',
        object_manager: false,
    )
    upowerd_dbus_sources += t[0]
    upowerd_dbus_headers += t[1]

    install_data(xml,
        install_dir: dbusdir / 'interfaces',
    )
endforeach


upowerd_dbus = static_library('libupower-dbus',
    sources: upowerd_dbus_sources + upowerd_dbus_headers,
    dependencies: [ gobject_dep, gio_dep, gio_unix_dep ],
)

upowerd_dbus_dep = declare_dependency(
    link_with: upowerd_dbus,
    include_directories: [ '.' ],
    sources: upowerd_dbus_headers,
    dependencies: [ gio_unix_dep ]
)
