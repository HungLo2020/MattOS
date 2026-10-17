#!/bin/sh
set -eu
# Establish persistent distribution migration state on fresh and legacy systems.
# The coordinator records the original release and this script's checksum.
install -d -m 0755 /var/lib/mattos/releases
