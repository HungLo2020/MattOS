# Make gzip/lib				-*-Makefile-*-

# Copyright (C) 2006, 2009-2025 Free Software Foundation, Inc.

# This program is free software; you can redistribute it and/or modify
# it under the terms of the GNU General Public License as published by
# the Free Software Foundation; either version 3 of the License, or
# (at your option) any later version.

# This program is distributed in the hope that it will be useful,
# but WITHOUT ANY WARRANTY; without even the implied warranty of
# MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
# GNU General Public License for more details.

# You should have received a copy of the GNU General Public License
# along with this program.  If not, see <https://www.gnu.org/licenses/>.

AM_CFLAGS =
BUILT_SOURCES =
CLEANFILES =
EXTRA_DIST =
MOSTLYCLEANDIRS =
MOSTLYCLEANFILES =
MAINTAINERCLEANFILES =
SUFFIXES =
noinst_LIBRARIES =

include gnulib.mk

libgzip_a_LIBADD += $(LIBOBJS)
libgzip_a_DEPENDENCIES += $(LIBOBJS)
AM_CFLAGS += $(GNULIB_WARN_CFLAGS) $(WERROR_CFLAGS)

match.$(OBJEXT): match.c
	$(AM_V_GEN)cp $(srcdir)/match.c _match.S
	$(AM_V_at)$(CPP) $(CPPFLAGS) $(ASCPPFLAGS) _match.S > _match.i
	$(AM_V_at)$(ASCPPPOST) < _match.i > match_.s
	$(AM_V_at)$(CC) $(CPPFLAGS) $(CFLAGS) $(ASFLAGS_config) -c match_.s
	$(AM_V_at)mv match_.$(OBJEXT) $@
	$(AM_V_at)rm -f _match.S _match.i match_.s

MOSTLYCLEANFILES += _match.S _match.i match_.s
