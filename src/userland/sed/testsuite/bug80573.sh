#!/bin/sh
# sed-4.9 and prior would leak a single byte in this one contrived case.

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

require_valgrind_

echo > in || framework_failure_
valgrind --quiet --error-exitcode=1 --leak-check=full \
	 --errors-for-leak-kinds=definite \
  sed -e x -e 's/.*/echo hi/e' in || fail=1

exit $fail
