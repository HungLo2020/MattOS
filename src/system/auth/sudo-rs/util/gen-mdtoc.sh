#! /bin/bash

FILE="${1:-FAQ.md}"

paste \
    <(grep '^##' "$FILE" | sed '
       s/##/-/
       :l
       s/-#/  -/; tl
    ') \
    <(sed -rn '/^##+/s/^#* *//p' "$FILE" | tr '[[:upper:]] ' '[[:lower:]]-' | tr -cd '[[:alnum:][:space:]-]' ) \
    | sed 's/\([ -]*\)\(.*\)\t\(.*\)/\1[\2](#\3)/'
