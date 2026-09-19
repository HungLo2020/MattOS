upshared_common = static_library('upshared-common',
    sources: [
        'up-backend-common.c',
    ],
    c_args: [ '-DG_LOG_DOMAIN="UPower-Unix"' ],
    dependencies: [ gudev_dep, upowerd_deps ],
    build_by_default: false,
)

upshared_common_dep = declare_dependency(
    link_with: [ upshared_common ],
    # TODO: Move up-backend-bsd-private.h here and add it as include directory
)
