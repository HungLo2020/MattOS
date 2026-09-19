#!/bin/sh

/usr/bin/scan-build-20 \
    -load-plugin /usr/lib64/tartan/20.1/libtartan.so \
    -disable-checker core.CallAndMessage \
    -disable-checker core.NullDereference \
    -disable-checker deadcode.DeadStores \
    -disable-checker unix.Malloc \
    -enable-checker tartan.GErrorChecker \
    --exclude meson-private \
    --status-bugs -v "$@"
