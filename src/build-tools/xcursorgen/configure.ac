dnl Process this file with autoconf to create configure.

# Initialize Autoconf
AC_PREREQ([2.60])
AC_INIT([xcursorgen],
	[1.0.9],
	[https://gitlab.freedesktop.org/xorg/app/xcursorgen/-/issues],
	[xcursorgen])
AC_CONFIG_SRCDIR([Makefile.am])
AC_CONFIG_HEADERS([config.h])
AC_USE_SYSTEM_EXTENSIONS

# Initialize Automake
AM_INIT_AUTOMAKE([foreign dist-xz])

# Require xorg-macros 1.8 or later: XORG_MANPAGE_SECTIONS
m4_ifndef([XORG_MACROS_VERSION],
	  [m4_fatal([must install xorg-macros 1.8 or later before running autoconf/autogen])])
XORG_MACROS_VERSION(1.8)
XORG_DEFAULT_OPTIONS

# Checks for pkg-config packages
PKG_CHECK_MODULES(XCURSORGEN, [x11 xcursor libpng >= 1.2.0 xproto >= 7.0.22])

AC_CHECK_FUNCS([asprintf])

AC_CONFIG_FILES([
	Makefile
	man/Makefile])
AC_OUTPUT
