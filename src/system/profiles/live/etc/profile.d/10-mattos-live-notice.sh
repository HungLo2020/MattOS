#!/bin/sh
if [ "${USER:-}" = "mattos" ] && [ -f /etc/motd ]; then
    cat /etc/motd
fi
