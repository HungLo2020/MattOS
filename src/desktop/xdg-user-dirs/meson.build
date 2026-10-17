project('xdg-user-dirs', 'c',
  meson_version: '>= 1.0',
  default_options: [
    'c_std=gnu11',
  ],

  version: '0.20',
  license: 'GPL-2.0-or-later',
)

cc = meson.get_compiler('c')

#
# Dependencies
#

# Gettext/libintl
libintl_dep = dependency('', required: false)
if cc.has_function('ngettext')
  libintl_dep = declare_dependency()
else
  libintl_dep = cc.find_library('intl', required: true)
endif

systemd_dep = dependency('systemd', required: false)

#
# Configuration data
#
conf_data = configuration_data()
conf_data.set_quoted('PACKAGE', meson.project_name())
conf_data.set_quoted('VERSION', meson.project_version())
conf_data.set_quoted('GETTEXT_PACKAGE', meson.project_name())
conf_data.set('HAVE_CONFIG_H', 1)

# Check for iconv const qualifier
iconv_const_check = '''
#include <iconv.h>
int main() {
    iconv_t cd = iconv_open("UTF-8", "ISO-8859-1");
    char *inbuf;
    char *outbuf;
    size_t inbytesleft, outbytesleft;
    iconv(cd, (const char **)&inbuf, &inbytesleft, &outbuf, &outbytesleft);
    return 0;
}
'''

if cc.compiles(iconv_const_check, name: 'iconv with const')
  conf_data.set('ICONV_CONST', 'const')
else
  conf_data.set('ICONV_CONST', '')
endif

# Set install directories
conf_data.set_quoted('XDGCONFDIR', get_option('sysconfdir') / 'xdg')
conf_data.set_quoted('LOCALEDIR', get_option('localedir'))

# Generate config.h
configure_file(
  output: 'config.h',
  configuration: conf_data
)

# Add config.h include directory
config_inc = include_directories('.')

#
# Compiler flags
#
add_project_arguments([
  '-DHAVE_CONFIG_H',
  '-Wchar-subscripts',
  '-Wmissing-declarations',
  '-Wmissing-prototypes',
  '-Wnested-externs',
  '-Wpointer-arith',
  '-Wcast-align',
  '-Wno-strict-aliasing',
  '-Wno-sign-compare',
  '-Wno-unused-function',
], language : 'c')

#
# Subdirs
#
subdir('src')
subdir('data')
if get_option('docs')
  subdir('man')
endif
if get_option('nls')
  subdir('po')
endif
