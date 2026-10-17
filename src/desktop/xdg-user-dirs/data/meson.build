
xdg_config_dir = get_option('sysconfdir') / 'xdg'
install_data(['user-dirs.conf', 'user-dirs.defaults'],
  install_dir: xdg_config_dir
)

xdg_autostart_dir = xdg_config_dir / 'autostart'
install_data('xdg-user-dirs.desktop',
  install_dir: xdg_autostart_dir
)

if systemd_dep.found()
  systemd_user_unit_dir = systemd_dep.get_variable(pkgconfig: 'systemduserunitdir')
else
  systemd_user_unit_dir = get_option('prefix') / 'lib' / 'systemd' / 'user'
endif

configure_file(
  input: 'xdg-user-dirs.service.in',
  output: '@BASENAME@',
  configuration: {
    'bindir': get_option('prefix') / get_option('bindir'),
  },
  install_dir: systemd_user_unit_dir,
)
