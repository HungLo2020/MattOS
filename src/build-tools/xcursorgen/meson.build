# SPDX-License-Identifier: MIT
#
# Copyright (c) 2025, Oracle and/or its affiliates.
#
# Permission is hereby granted, free of charge, to any person obtaining a
# copy of this software and associated documentation files (the "Software"),
# to deal in the Software without restriction, including without limitation
# the rights to use, copy, modify, merge, publish, distribute, sublicense,
# and/or sell copies of the Software, and to permit persons to whom the
# Software is furnished to do so, subject to the following conditions:
#
# The above copyright notice and this permission notice (including the next
# paragraph) shall be included in all copies or substantial portions of the
# Software.
#
# THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
# IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
# FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT.  IN NO EVENT SHALL
# THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
# LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
# FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
# DEALINGS IN THE SOFTWARE.
#

project(
  'xcursorgen',
  'c',
  version: '1.0.9',
  license: 'HPND-sell-variant',
  license_files: 'COPYING',
  meson_version: '>= 1.1.0',
)

cc = meson.get_compiler('c')

conf = configuration_data()
conf.set_quoted('PACKAGE_VERSION', meson.project_version())

# Replaces AC_USE_SYSTEM_EXTENSIONS
if host_machine.system() == 'sunos'
    system_extensions = '__EXTENSIONS__'
elif host_machine.system() == 'netbsd'
    system_extensions = '_NETBSD_SOURCE'
else
    system_extensions = '_GNU_SOURCE'
endif
conf.set(system_extensions, 1,
         description: 'Enable non-standardized system API extensions')

# Replacement for XORG_DEFAULT_OPTIONS
if cc.has_argument('-fno-strict-aliasing')
  add_project_arguments('-fno-strict-aliasing', language: 'c')
endif

# Checks for pkg-config packages
dep_libx11     = dependency('x11', required: true)
dep_libxcursor = dependency('xcursor', required: true)
dep_libpng     = dependency('libpng', required: true, version: '>= 1.2.0')
dep_xproto     = dependency('xproto', required: true, version: '>= 7.0.22')

conf.set('HAVE_ASPRINTF', cc.has_function('asprintf') ? '1' : false)

config_h = configure_file(output: 'config.h', configuration: conf)
add_project_arguments('-DHAVE_CONFIG_H', language: ['c'])

executable(
  'xcursorgen',
  [config_h, 'xcursorgen.c'],
  dependencies: [dep_libx11, dep_libxcursor, dep_libpng, dep_xproto],
  install: true
)

# Man page
prog_sed = find_program('sed')

custom_target(
  'xcursorgen.man',
  input: 'man/xcursorgen.man',
  output: 'xcursorgen.1',
  command: [
    prog_sed,
    '-e', 's/__vendorversion__/"xcursorgen @0@" "X Version 11"/'.format(meson.project_version()),
    '-e', 's/__appmansuffix__/1/g',
    '-e', 's/__libmansuffix__/3/g',
    '@INPUT@',
  ],
  capture: true,
  install: true,
  install_dir: get_option('mandir') / 'man1',
)
