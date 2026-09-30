#!/bin/sh
# Ensure that a** works in BRE mode (GNU extension)

# Copyright (C) 2026 Free Software Foundation, Inc.

# This program is free software: you can redistribute it and/or modify
# it under the terms of the GNU General Public License as published by
# the Free Software Foundation, either version 3 of the License, or
# (at your option) any later version.

# This program is distributed in the hope that it will be useful,
# but WITHOUT ANY WARRANTY; without even the implied warranty of
# MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
# GNU General Public License for more details.

# You should have received a copy of the GNU General Public License
# along with this program.  If not, see <https://www.gnu.org/licenses/>.
. "${srcdir=.}/testsuite/init.sh"; path_prepend_ ./sed
print_ver_ sed

unset POSIXLY_CORRECT
export LC_ALL=C

# a** is a documented GNU extension meaning the same as a*
# and must work in BRE mode, not just in ERE mode.
printf '%s\n' 10 11 12 13 14 15 16 17 18 19 20 > in || framework_failure_
printf '%s\n' 0 '' 2 3 4 5 6 7 8 9 20 > exp || framework_failure_

# BRE: was rejected before this fix
sed 's/1**//' in > out-bre || fail=1
compare_ exp out-bre || fail=1

# ERE: always worked
sed -E 's/1**//' in > out-ere || fail=1
compare_ exp out-ere || fail=1

# --posix BRE: must reject ** (not a POSIX feature)
cat <<\EOF > exp-err || framework_failure_
sed: -e expression #1, char 7: Invalid preceding regular expression
EOF
returns_ 1 sed --posix 's/1**//' in 2>err || fail=1
compare_ exp-err err || fail=1

Exit $fail
