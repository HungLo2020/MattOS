zshcompletiondir = get_option('zshcompletiondir')
if zshcompletiondir == ''
  zshcompletiondir = get_option('datadir') / 'zsh' / 'site-functions'
endif

if zshcompletiondir != 'no'
  install_data(
    '_upower',
    install_dir: zshcompletiondir,
    install_mode: 'rw-r--r--',
  )
endif
