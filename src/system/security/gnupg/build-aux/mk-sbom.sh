#!/bin/sh
# mk-sbom.sh - script to create info for an SBOM
# Copyright (C) 2024 g10 Code GmbH
#
# This file is free software; as a special exception the author gives
# unlimited permission to copy and/or distribute it, with or without
# modifications, as long as this notice is preserved.
#
# This program is distributed in the hope that it will be useful, but
# WITHOUT ANY WARRANTY, to the extent permitted by law; without even the
# implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.
# SPDX-License-Identifier: FSFULLRWD

# Note : This script will eventually be turned into a a more versatile
#        SBOM generator for GnuPG related packages.


set -e

PGMDATE=2026-06-29
PGM=mk-sbom.sh

usage()
{
    cat <<EOF
Usage: $PGM [OPTIONS] package_files
Output a line for each package with information useful for a SBOM.

Options:
        --name=NAME     Override package_file by NAME
        --uncompressor=TOOL  Use TOOL for uncompressing
        --patch         Expect patch files instead of tarballs
        --verbose       Show how the commit-id was retrieved
        --version       Print version of this tool
        --help          This output.

Example:
    ./$PGM foo-2.3.tar.bz2 bar-1.2.3.tar.xz

        Output two lines with the checksum, name and version, and the
        commit-id (if available).
EOF
    exit $1
}

# Parse the command line options.
name=
uncompressor=
patchmode=
verbose=
while [ $# -gt 0 ]; do
    case "$1" in
	--*=*)
	    optarg=`echo "$1" | sed 's/[-_a-zA-Z0-9]*=//'`
	    ;;
	*)
	    optarg=""
	    ;;
    esac
    case "$1" in
        --name=*)
	    name="$optarg"
	    ;;
        --uncompressor=*)
	    uncompressor="$optarg"
	    ;;
        --patch) patchmode=yes ;;
        --verbose|-v) verbose=yes ;;
        --help|-h) usage 0 ;;
        --version) echo "$PGM $PGMDATE"; exit 0 ;;
        --) shift; break 2 ;;
        --*) usage 1 1>&2; exit 1 ;;
        *) break 2 ;;
    esac
    shift
done

if [ -n "$name" ] && [ $# -gt 1 ]; then
    echo >&2 "$PGM: error: only one file allowed with --name"
    exit 2
fi

if [ -n "$uncompressor" ]; then
    case "$uncompressor" in
        zcat|bzcat|xzcat|cat)
          ;;
        *)
          echo >&2 "$PGM: error: invalid value for --uncompressor"
          exit 2
    esac
fi

for file in "$@"; do
    sha1=$(sha1sum < "$file"|cut -f1 -d' ')
    if [ -z "$name" ]; then
        name=$(echo "${file##*/}" | \
               sed -E 's/\.(tar\.(gz|bz2|xz)|tar|zip|tgz|tbz|tbz2)$//')
    fi
    commitid=
    uncompress=
    if [ -n "$uncompressor" ]; then
        uncompress=$uncompressor
    else
        case "${file##*/}" in
            *.tar.gz|*.tgz)         uncompress=zcat ;;
            *.tar.bz2|*.tbz2|*.tbz) uncompress=bzcat ;;
	    *.tar.xz)               uncompress=xzcat ;;
	    *.tar)                  uncompress=cat ;;
        esac
    fi
    if [ -n "$uncompress" ] && [ -z "$patchmode" ]; then
        source="tar comment"
        commitid=$($uncompress "$file" 2>/dev/null \
                      | git get-tar-commit-id 2>/dev/null || true)
        if [ -z "$commitid" ]; then
            source="version file"
            save_IFS="$IFS"
            IFS=
            verfile=$($uncompress "$file" 2>/dev/null \
                      | tar -xOf - --wildcards "*/VERSION" 2>/dev/null || true)
            IFS="$save_IFS"
            commitid=$(echo "$verfile" | sed -n 2p)
        fi
        if [ -n "$commitid" -a $(echo -n "$commitid" | wc -c) -ne 40 ]; then
            echo >&2 "$PGM: bad commit-id in '$file' ($source)"
            commitid=
        fi
    else
        source="n/a"
    fi
    if [ -z "$verbose" ]; then
       :
    elif [ -n "$commitid" ]; then
        echo >&2 "$PGM: $file - commit-id from $source"
    else
        echo >&2 "$PGM: $file"
    fi
    echo "$sha1 $name ${commitid:--}"
done
