// Native hardware audio dependencies. All outputs belong to their producer.
fn build_alsa_lib(repo_root: &Path) -> Result<()> {
    build_autotools_import(
        repo_root, "alsa-lib", "src/system/multimedia/alsa-lib", &[],
        &["--prefix=/usr", "--libdir=/usr/lib/x86_64-linux-gnu",
          "--disable-static", "--disable-python", "--disable-alisp"],
        &["usr/lib/x86_64-linux-gnu/libasound.so.2", "usr/share/alsa/alsa.conf"],
    )?;
    remove_staged_libtool_archives(&repo_root.join("out/build/alsa-lib/install"))
}

fn build_alsa_ucm_conf(repo_root: &Path) -> Result<()> {
    let install = repo_root.join("out/build/alsa-ucm-conf/install");
    remove_path_if_exists(&install)?;
    for directory in ["ucm", "ucm2"] {
        packaging::copy_tree_preserving(
            &repo_root.join("src/system/multimedia/alsa-ucm-conf").join(directory),
            &install.join("usr/share/alsa").join(directory),
        )?;
    }
    Ok(())
}

fn build_alsa_utils(repo_root: &Path) -> Result<()> {
    build_release_autotools_program(
        repo_root, "alsa-utils", "alsa-utils-1.2.16.tar.bz2",
        ALSA_UTILS_RELEASE_ARCHIVE_URL, ALSA_UTILS_RELEASE_ARCHIVE_SHA256,
        &["alsa-lib", "ncurses", "systemd"],
        &["--prefix=/usr", "--sbindir=/usr/sbin", "--sysconfdir=/etc",
          "--localstatedir=/var", "--libdir=/usr/lib/x86_64-linux-gnu",
          "--with-curses=ncursesw", "--disable-alsaconf", "--disable-alsaloop",
          "--disable-bat", "--disable-nls", "--disable-maintainer-mode", "--disable-xmlto", "--disable-rst2man",
          "--with-systemdsystemunitdir=/usr/lib/systemd/system",
          "--with-udev-rules-dir=/usr/lib/udev/rules.d"],
        &["usr/bin/aplay", "usr/bin/amixer", "usr/sbin/alsactl",
          "usr/lib/systemd/system/alsa-restore.service"],
    )?;
    remove_staged_libtool_archives(&repo_root.join("out/build/alsa-utils/install"))
}

fn build_sbc(repo_root: &Path) -> Result<()> {
    build_autotools_import(
        repo_root, "sbc", "src/system/multimedia/sbc", &[],
        &["--prefix=/usr", "--libdir=/usr/lib/x86_64-linux-gnu",
          "--disable-static", "--disable-tools", "--disable-tester"],
        &["usr/lib/x86_64-linux-gnu/libsbc.so.1"],
    )?;
    remove_staged_libtool_archives(&repo_root.join("out/build/sbc/install"))
}
