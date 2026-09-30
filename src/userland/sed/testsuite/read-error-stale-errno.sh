#!/bin/sh
# Verify ck_getdelim no longer reports a misleading strerror(errno)
# when ferror is already set from a prior I/O operation.

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

# Verify that non-blocking read from a FIFO with no data (but a writer
# still attached) fails with EAGAIN rather than returning 0 (EOF).
# Cygwin's FIFO implementation gets this wrong, violating POSIX.
mkfifo probe || framework_failure_
{ exec sleep 99; } > probe &
probe_pid=$!
(exec < probe; dd iflag=nonblock bs=1 count=1 2>/dev/null)
probe_rc=$?
kill $probe_pid 2>/dev/null; wait
rm -f probe
test $probe_rc = 0 \
  && skip_ "FIFO+O_NONBLOCK returns EOF, not EAGAIN"

# Use a FIFO with non-blocking I/O.  After sed reads one line,
# the next read(2) fails with EAGAIN (not EOF, since a writer is
# still attached), setting ferror on the stream.
#
# With sed -s, test_eof's getc triggers EAGAIN, setting ferror;
# ck_getdelim is then called with ferror already set.
#
# Before the fix: "sed: read error on stdin: Resource temporarily unavailable"
# After the fix:  "sed: read error on stdin"

mkfifo fifo || framework_failure_

# A writer that keeps the FIFO's write-end open.
{ printf 'hello\n'; exec sleep 99; } > fifo &
writer_pid=$!

# "dd iflag=nonblock count=0" sets O_NONBLOCK on the file description
# underlying stdin (a no-op read that just changes the flag).
# That flag is then inherited by sed.
# Skip if dd doesn't support iflag=nonblock.
(
  exec < fifo
  dd iflag=nonblock count=0 2>/dev/null || exit 77
  sed -s -n '$p'
) 2>err
rc=$?

kill $writer_pid 2>/dev/null
wait

test $rc -eq 77 && skip_ "dd does not support iflag=nonblock"

# sed should have panicked (exit 4) due to the read error.
test $rc -eq 4 || fail=1

# The error message must mention "read error" ...
grep 'read error' err > /dev/null || fail=1

# ... but must NOT include a ": strerror" suffix, which would be misleading
# (before the fix, this was e.g., ": Resource temporarily unavailable").
grep 'read error.*: ' err && fail=1

Exit $fail
