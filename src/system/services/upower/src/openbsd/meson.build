upshared += { 'openbsd': static_library('upshared',
    sources: [
        'up-backend.c',
        'up-native.c',
        'up-apm-native.h',
    ],
    c_args: [ '-DG_LOG_DOMAIN="UPower-Openbsd"' ],
    dependencies: [ gudev_dep, upowerd_deps, upshared_common_dep ],
    build_by_default: false,
)}
