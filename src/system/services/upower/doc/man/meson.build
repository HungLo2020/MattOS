
man_pages = [
    [ 'upower', 1 ],
    [ 'upowerd', 8 ],
    [ 'UPower', 7 ],
]

foreach man: man_pages
    custom_target(
        '@0@.@1@'.format(man[0], man[1]),
        input: files(join_paths(man[0] + '.xml')),
        output: '@0@.@1@'.format(man[0], man[1]),
        command: [xsltproc, '--output', '@OUTPUT@', '-nonet', 'http://docbook.sourceforge.net/release/xsl/current/manpages/docbook.xsl', '@INPUT@'],
        build_by_default: get_option('man'),
        install: get_option('man'),
        install_dir: get_option('prefix') / get_option('mandir') / 'man@0@'.format(man[1]),
    )
endforeach
