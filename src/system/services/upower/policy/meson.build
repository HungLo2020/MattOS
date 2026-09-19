if polkit.found()
  install_data(
    'org.freedesktop.upower.rules',
    install_tag: 'runtime',
    install_dir: join_paths(datadir, 'polkit-1', 'rules.d'),
  )

  #newer polkit has the ITS rules included
  if polkit.version().version_compare('>0.113')
    i18n.merge_file(
      input: 'org.freedesktop.upower.policy.in',
      output: 'org.freedesktop.upower.policy',
      install: true,
      install_dir: join_paths(datadir, 'polkit-1', 'actions') ,
      type: 'xml',
      po_dir: join_paths(meson.project_source_root(), 'po')
    )
  #older polkit is missing ITS rules and will fail
  else
    i18n.merge_file(
      input: 'org.freedesktop.upower.policy.in',
      output: 'org.freedesktop.upower.policy',
      install: true,
      install_dir: join_paths(datadir, 'polkit-1', 'actions') ,
      type: 'xml',
      data_dirs: join_paths(meson.project_source_root(), 'policy'),
      po_dir: join_paths(meson.project_source_root(), 'po')
    )
  endif
endif