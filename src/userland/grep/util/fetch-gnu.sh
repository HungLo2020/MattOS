#!/bin/bash -e
# This file is part of the uutils grep package.
#
# For the full copyright and license information, please view the LICENSE
# file that was distributed with this source code.
#
# Download and extract the upstream GNU grep release tarball into the current
# directory. Run it from an (empty) directory that will hold the GNU grep tree,
# e.g.:
#
#   mkdir -p ../gnu.grep && (cd ../gnu.grep && bash ../grep/util/fetch-gnu.sh)
#
# The extracted tree ships a ready-to-use gnulib test framework under tests/
# (init.sh + init.cfg + the extensionless test scripts), which
# util/run-gnu-testsuite.sh drives against the Rust grep binary.
ver="3.12"
curl -L "https://ftp.gnu.org/gnu/grep/grep-${ver}.tar.xz" | tar --strip-components=1 -xJf -
