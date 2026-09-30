#!/bin/sh
# Ensure --debug doesn't crash with labels compiled before --debug is set

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

# When --debug appears after -f, the label command is compiled
# with debug=false, so label_name is not set.  This must not crash.
printf ':label\n' > prog || framework_failure_

sed -f prog --debug < /dev/null > out 2>&1 || fail=1

# The label name may be absent (compiled without debug), but no crash.
grep '^SED PROGRAM:' out > /dev/null || fail=1

# When --debug appears before -f, the label name should be printed.
sed --debug -f prog < /dev/null > out2 2>&1 || fail=1
grep ':label' out2 > /dev/null || fail=1

Exit $fail
