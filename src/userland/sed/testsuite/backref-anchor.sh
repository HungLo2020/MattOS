#! /bin/sh
# Test for a backref+anchor bug inherited from glibc's regex.
#
# Copyright 2026 Free Software Foundation, Inc.
#
# Copying and distribution of this file, with or without modification,
# are permitted in any medium without royalty provided the copyright
# notice and this notice are preserved.

. "${srcdir=.}/testsuite/init.sh"; path_prepend_ ./sed

echo ab > in || framework_failure_

# Expect no match.
sed -E 's/^(.?)(.?).?\2\1$/X/' in > out 2> err || fail=1
compare in out || fail=1
compare /dev/null err || fail=1

# Match "a"
echo Xb > exp || framework_failure_
sed -E 's/^(.?)(.?).?\2\1/X/' in > out 2> err || fail=1
compare exp out || fail=1
compare /dev/null err || fail=1

Exit $fail
