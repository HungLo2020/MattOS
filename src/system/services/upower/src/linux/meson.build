idevice_sources = []
if idevice_dep.found()
    idevice_sources = [
        'up-device-idevice.c',
        'up-device-idevice.h',
    ]
endif

upshared += { 'linux': static_library('upshared',
    sources: [
        'up-device-supply.c',
        'up-device-supply.h',
        'up-device-supply-battery.c',
        'up-device-supply-battery.h',
        'up-device-hid.c',
        'up-device-hid.h',
        'up-device-wup.c',
        'up-device-wup.h',
        'up-device-bluez.c',
        'up-device-bluez.h',
        'up-kbd-backlight-led.c',
        'up-kbd-backlight-led.h',
        'up-input.c',
        'up-input.h',
        'up-backend.c',
        'up-native.c',
        'up-enumerator-udev.c',
        idevice_sources
    ],
    c_args: [ '-DG_LOG_DOMAIN="UPower-Linux"' ],
    dependencies: [ gudev_dep, upowerd_deps, idevice_dep, plist_dep ],
    build_by_default: false,
)}
