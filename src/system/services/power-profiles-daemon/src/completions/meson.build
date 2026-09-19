generate_completion = [python3, powerprofilesctl, '--print-completion']

completions_common = {
  'env': {'PPD_COMPLETIONS_GENERATION': '1'},
  'capture': true,
  'install': true,
}

if bashcomp.found()
  completions_dir = bashcomp.get_variable(pkgconfig: 'completionsdir',
    pkgconfig_define: bashcomp.version().version_compare('>= 2.10') ?
      ['datadir', get_option('datadir')] : ['prefix', prefix],
  )

  custom_target('bash-completion',
    output: 'powerprofilesctl',
    command: [
      generate_completion,
      'bash',
    ],
    install_dir: completions_dir,
    kwargs: completions_common,
  )
endif

if zshcomp
  custom_target('zsh-completion',
    output: '_powerprofilesctl',
    command: [
      generate_completion,
      'zsh',
    ],
    install_dir: get_option('zshcomp'),
    kwargs: completions_common,
  )
endif
