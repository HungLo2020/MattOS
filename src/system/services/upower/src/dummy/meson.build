upshared += { 'dummy': static_library('upshared',
    sources: [
        'up-backend.c',
        'up-native.c',
    ],
    c_args: [ '-DG_LOG_DOMAIN="UPower-Dummy"' ],
    dependencies: [ gudev_dep, upowerd_deps ],
    build_by_default: false,
)}
