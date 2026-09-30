#!/bin/sh
# Ensure \n, \t, etc. after a named character class work with --posix

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

export LC_ALL=C

# The bug: in --posix mode, normalize_text tracks bracket state using
# p[-1] and p[-2], but when an earlier \n was collapsed from 2 bytes
# to 1, those lookbacks read overwritten data.  This left bracket_state
# stuck, preventing later \n from being converted to a newline.

printf '%s\n' A B C > in || framework_failure_
printf 'XXXC\n' > exp || framework_failure_

# The original bug report: [[:alpha:]] followed by \n*
sed --posix -E 'N; N; s/^A\n[[:alpha:]]\n*/XXX/' in > out1 2>/dev/null || fail=1
compare_ exp out1 || fail=1

# Other named classes must also work.
printf '%s\n' A 1 C > in2 || framework_failure_
sed --posix -E 'N; N; s/^A\n[[:digit:]]\n*/XXX/' in2 > out2 2>/dev/null || fail=1
compare_ exp out2 || fail=1

sed --posix -E 'N; N; s/^A\n[[:upper:]]\n*/XXX/' in > out3 2>/dev/null || fail=1
compare_ exp out3 || fail=1

# Other escapes (\t, not just \n) are also affected.
printf 'A\nB\tC\n' > in3 || framework_failure_
sed --posix -E 'N; s/^A\n[[:alpha:]]\t/XXX/' in3 > out4 2>/dev/null || fail=1
compare_ exp out4 || fail=1

# Without --posix, it always worked; verify no regression.
sed -E 'N; N; s/^A\n[[:alpha:]]\n*/XXX/' in > out5 || fail=1
compare_ exp out5 || fail=1

Exit $fail
