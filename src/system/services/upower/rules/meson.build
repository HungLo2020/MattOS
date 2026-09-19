rules = [
    '60-upower-battery.rules',
    '95-upower-wup.rules',
    '95-upower-hid.rules',
]

hwdb = [
    '95-upower-hid.hwdb',
    '60-upower-battery.hwdb',
]

if os_backend == 'linux'
    install_data(
        rules,
        install_dir: udevrulesdir,
    )

    install_data(
        hwdb,
        install_dir: udevhwdbdir,
    )
endif
