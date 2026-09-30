fn build_pkgconf(repo_root: &Path) -> Result<()> {
    // libpkgconf is linked statically: MattOS ships one pkgconf package
    // rather than Debian's pkgconf/pkgconf-bin/libpkgconf3 split.
    build_meson_runtime(
        repo_root,
        "pkgconf",
        "src/build-tools/pkgconf",
        &[],
        &[
            "--prefix=/usr",
            "--libdir=lib/x86_64-linux-gnu",
            "--buildtype=release",
            "-Ddefault_library=static",
            "-Dwith-pkg-config-dir=/usr/local/lib/x86_64-linux-gnu/pkgconfig:/usr/local/lib/pkgconfig:/usr/local/share/pkgconfig:/usr/lib/x86_64-linux-gnu/pkgconfig:/usr/lib/pkgconfig:/usr/share/pkgconfig",
            "-Dwith-system-libdir=/usr/lib/x86_64-linux-gnu:/usr/lib:/lib",
            "-Dwith-system-includedir=/usr/include",
        ],
        "usr/bin/pkgconf",
        &[],
    )
}

fn build_cmake(repo_root: &Path) -> Result<()> {
    // CMake's own bundled third-party libraries (curl, libarchive, zlib, ...)
    // are part of the vendored tree; only OpenSSL comes from MattOS, named
    // explicitly so FindOpenSSL can never select the build host's copy.
    let openssl = repo_root.join("out/build/openssl/install/usr");
    let include = format!("-DOPENSSL_INCLUDE_DIR={}", openssl.join("include").display());
    let crypto = format!(
        "-DOPENSSL_CRYPTO_LIBRARY={}",
        openssl.join("lib/x86_64-linux-gnu/libcrypto.so").display()
    );
    let ssl = format!(
        "-DOPENSSL_SSL_LIBRARY={}",
        openssl.join("lib/x86_64-linux-gnu/libssl.so").display()
    );
    // Everything else is found only inside the MattOS sysroot, never on the
    // build host (the bundled curl otherwise probes for host libidn2).
    let root = format!("-DCMAKE_FIND_ROOT_PATH={}", repo_root.join("out/sysroot").display());
    build_cmake_runtime(
        repo_root,
        "cmake",
        "src/build-tools/cmake",
        &["openssl"],
        &[
            &root,
            "-DCMAKE_FIND_ROOT_PATH_MODE_PROGRAM=NEVER",
            "-DCMAKE_FIND_ROOT_PATH_MODE_LIBRARY=ONLY",
            "-DCMAKE_FIND_ROOT_PATH_MODE_INCLUDE=ONLY",
            "-DCMAKE_FIND_ROOT_PATH_MODE_PACKAGE=ONLY",
            "-DUSE_LIBIDN2=OFF",
            "-DCURL_USE_LIBPSL=OFF",
            "-DCURL_USE_LIBSSH2=OFF",
            "-DCURL_BROTLI=OFF",
            "-DCURL_USE_GSSAPI=OFF",
            "-DCMAKE_INSTALL_PREFIX=/usr",
            "-DCMAKE_BUILD_TYPE=Release",
            "-DCMAKE_USE_SYSTEM_LIBRARIES=OFF",
            "-DCMAKE_USE_OPENSSL=ON",
            &include,
            &crypto,
            &ssl,
            "-DBUILD_TESTING=OFF",
            "-DBUILD_CursesDialog=OFF",
            "-DBUILD_QtDialog=OFF",
            "-DSPHINX_MAN=OFF",
            "-DSPHINX_HTML=OFF",
            "-DCMake_BUILD_LTO=OFF",
        ],
        "usr/bin/cmake",
    )
}
