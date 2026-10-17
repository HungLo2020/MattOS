
xdg_user_dirs_update = executable('xdg-user-dirs-update',
  'xdg-user-dirs-update.c',
  include_directories: config_inc,
  dependencies: [libintl_dep],
  install: true
)

xdg_user_dir = executable('xdg-user-dir',
  [
    'xdg-user-dir-lookup.h',
    'xdg-user-dir-lookup.c',
    'xdg-user-dir.c',
  ],
  include_directories: config_inc,
  install: true
)
