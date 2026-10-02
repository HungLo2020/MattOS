
if build_machine.system() != 'windows'
    install_man('update-mime-database.1')
endif

freedesktop_org_xml = i18n.merge_file(
    input: 'freedesktop.org.xml.in',
    output: 'freedesktop.org.xml',
    data_dirs: '.',
    po_dir: '../po',
    type: 'xml',
    install: true,
    install_dir: get_option('datadir') / 'mime' / 'packages',
)

install_data(
  [ 'its/shared-mime-info.loc', 'its/shared-mime-info.its', ],
  install_dir : get_option('datadir') / 'gettext/its'
)

if get_option('build-spec')
    custom_target('shared-mime-info-spec-html',
        input : 'shared-mime-info-spec.xml',
        output: 'shared-mime-info-spec-html',
        command: [
            xmlto,
            '-o', '@OUTPUT@',
            'html-nochunks',
            '@INPUT@',
        ],
        build_by_default: true,
)
endif
