#!/usr/bin/env bash
set -euo pipefail

: ${1:?filename argument missing}
xml_db_file="${1}"
test -f ${xml_db_file} || {
    printf "%s: no such file\n" ${xml_db_file} >&2
    exit 1
}

duplicated=$(
    xmllint --xpath \
            "//*[local-name()='mime-type' or local-name()='alias']/@type" \
            ${xml_db_file} | tr ' ' '\n' | sort | uniq -d
)

if [[ -n "${duplicated}" ]]; then
    echo "*************************************************************"
    echo "** Some mime-types are duplicated, fix before committing:  **"
    echo "${duplicated}"
    echo "*************************************************************"
    exit 1
fi
