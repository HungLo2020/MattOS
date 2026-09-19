// Runtime-facing multimedia and foundational library recipes.
// Included into the crate root to preserve existing helper visibility.
fn build_dav1d(repo_root: &Path) -> Result<()> {
    build_meson_runtime(
        repo_root,
        "dav1d",
        "src/system/multimedia/dav1d",
        &[],
        &[
            "--prefix=/usr",
            "--libdir=lib/x86_64-linux-gnu",
            "--buildtype=release",
            "-Denable_asm=false",
            "-Denable_tools=false",
            "-Denable_examples=false",
            "-Denable_tests=false",
            "-Denable_docs=false",
        ],
        "usr/lib/x86_64-linux-gnu/libdav1d.so.7",
        &[],
    )
}

fn build_glib(repo_root: &Path) -> Result<()> {
    build_meson_runtime(
        repo_root,
        "glib",
        "src/system/libraries/glib",
        &["libffi", "pcre2", "zlib"],
        &[
            "--prefix=/usr",
            "--libdir=lib/x86_64-linux-gnu",
            "--buildtype=release",
            "-Dtests=false",
            "-Dinstalled_tests=false",
            "-Dnls=disabled",
            "-Dselinux=disabled",
            "-Dlibmount=disabled",
            "-Dlibelf=disabled",
            "-Dintrospection=disabled",
            "-Dman-pages=disabled",
            "-Ddtrace=disabled",
            "-Dsystemtap=disabled",
            "-Dsysprof=disabled",
            "-Dglib_debug=disabled",
        ],
        "usr/lib/x86_64-linux-gnu/libglib-2.0.so.0",
        &[],
    )?;
    let glib_usr = repo_root.join("out/build/glib/install/usr");
    let glib_pc = glib_usr.join("lib/x86_64-linux-gnu/pkgconfig");
    rewrite_pkgconfig_prefixes(&glib_pc, &glib_usr)?;
    // GLib's public .pc files expose these private requirements even for a
    // dynamic consumer. Keep their development metadata in the same
    // output-owned SDK directory so pkg-config cannot fall back to the host.
    for (component, names) in [
        ("pcre2", &["libpcre2-8.pc"][..]),
        ("libffi", &["libffi.pc"][..]),
    ] {
        let dependency_usr = repo_root
            .join("out/build")
            .join(component)
            .join("install/usr");
        let dependency_pc = dependency_usr.join("lib/x86_64-linux-gnu/pkgconfig");
        for name in names {
            fs::copy(dependency_pc.join(name), glib_pc.join(name))?;
        }
        rewrite_selected_pkgconfig_prefixes(&glib_pc, names, &dependency_usr)?;
    }
    for required in [
        "usr/lib/x86_64-linux-gnu/libgobject-2.0.so.0",
        "usr/lib/x86_64-linux-gnu/libgio-2.0.so.0",
        "usr/bin/glib-compile-schemas",
    ] {
        if !repo_root
            .join("out/build/glib/install")
            .join(required)
            .is_file()
        {
            bail!("GLib build did not install /{required}");
        }
    }
    Ok(())
}

fn build_ffmpeg(repo_root: &Path) -> Result<()> {
    build_autotools_import(
        repo_root,
        "ffmpeg",
        "src/system/multimedia/ffmpeg",
        &[],
        &[
            "--prefix=/usr",
            "--libdir=/usr/lib/x86_64-linux-gnu",
            "--enable-shared",
            "--disable-static",
            "--disable-autodetect",
            "--disable-programs",
            "--disable-doc",
            "--disable-debug",
            "--disable-network",
            // NASM is a build-time optimization only. Keep this target-owned
            // library closure independent of an undeclared host assembler.
            "--disable-x86asm",
        ],
        &[
            "usr/lib/x86_64-linux-gnu/libavcodec.so",
            "usr/lib/x86_64-linux-gnu/libavformat.so",
            "usr/lib/x86_64-linux-gnu/libavfilter.so",
            "usr/lib/x86_64-linux-gnu/libavutil.so",
            "usr/lib/x86_64-linux-gnu/libswscale.so",
        ],
    )
}

fn build_libva(repo_root: &Path) -> Result<()> {
    build_meson_runtime(
        repo_root,
        "libva",
        "src/system/graphics/libva",
        &["libdrm", "wayland", "libffi"],
        &[
            "--prefix=/usr",
            "--libdir=lib/x86_64-linux-gnu",
            "--buildtype=release",
            "-Dwith_x11=no",
            "-Dwith_glx=no",
            "-Dwith_wayland=yes",
            "-Dwith_win32=no",
            "-Denable_docs=false",
        ],
        "usr/lib/x86_64-linux-gnu/libva.so",
        &[],
    )
}

fn build_opencv(repo_root: &Path) -> Result<()> {
    let source = repo_root.join("src/system/multimedia/opencv");
    let out_root = repo_root.join("out/build/opencv");
    let source_copy = out_root.join("source");
    let build = out_root.join("build");
    let install = out_root.join("install");
    let state = fs::read_to_string(repo_root.join("upstream/state/opencv.toml"))?;
    let zlib_usr = repo_root.join("out/build/zlib/install/usr");
    let mut options = vec![
        "-DCMAKE_INSTALL_PREFIX=/usr",
        "-DCMAKE_INSTALL_LIBDIR=lib/x86_64-linux-gnu",
        "-DCMAKE_BUILD_TYPE=Release",
        "-DBUILD_SHARED_LIBS=ON",
        "-DBUILD_LIST=core,imgproc",
        "-DBUILD_opencv_apps=OFF",
        "-DBUILD_TESTS=OFF",
        "-DBUILD_PERF_TESTS=OFF",
        "-DBUILD_EXAMPLES=OFF",
        "-DBUILD_DOCS=OFF",
        "-DBUILD_JAVA=OFF",
        "-DBUILD_opencv_python3=OFF",
        "-DBUILD_WITH_DEBUG_INFO=OFF",
        "-DOPENCV_GENERATE_PKGCONFIG=ON",
        "-DWITH_IPP=OFF",
        "-DWITH_OPENCL=OFF",
        "-DWITH_OPENEXR=OFF",
        "-DWITH_JPEG=OFF",
        "-DWITH_PNG=OFF",
        "-DWITH_TIFF=OFF",
        "-DWITH_WEBP=OFF",
        "-DWITH_FFMPEG=OFF",
        "-DWITH_GSTREAMER=OFF",
        "-DWITH_V4L=OFF",
        "-DWITH_GTK=OFF",
        "-DWITH_QT=OFF",
        "-DWITH_OPENGL=OFF",
        "-DCPU_BASELINE=SSE3",
        "-DCPU_DISPATCH=",
    ].into_iter().map(str::to_string).collect::<Vec<_>>();
    options.push(format!("-DZLIB_INCLUDE_DIR={}", zlib_usr.join("include").display()));
    options.push(format!("-DZLIB_INCLUDE_DIRS={}", zlib_usr.join("include").display()));
    options.push(format!("-DZLIB_LIBRARY={}", zlib_usr.join("lib/x86_64-linux-gnu/libz.so").display()));
    options.push(format!("-DZLIB_LIBRARIES={}", zlib_usr.join("lib/x86_64-linux-gnu/libz.so").display()));
    options.push(format!("-DCMAKE_C_STANDARD_INCLUDE_DIRECTORIES={}", zlib_usr.join("include").display()));
    options.push(format!("-DCMAKE_CXX_STANDARD_INCLUDE_DIRECTORIES={}", zlib_usr.join("include").display()));
    let source_map = format!(
        "-O2 -g0 -ffile-prefix-map={}=/usr/src/mattos/opencv -fdebug-prefix-map={}=/usr/src/mattos/opencv -fmacro-prefix-map={}=/usr/src/mattos/opencv -ffile-prefix-map={}=/usr/src/mattos/opencv-build -fdebug-prefix-map={}=/usr/src/mattos/opencv-build -fmacro-prefix-map={}=/usr/src/mattos/opencv-build",
        source_copy.display(),
        source_copy.display(),
        source_copy.display(),
        build.display(),
        build.display(),
        build.display()
    );
    options.push(format!("-DCMAKE_C_FLAGS={source_map}"));
    options.push(format!("-DCMAKE_CXX_FLAGS={source_map}"));
    options.push(format!("-DMATTOS_BUILD_ROOT={}", repo_root.display()));
    let stamp = format!("{state}\n{}\ndependencies=zlib\n", options.join("\n"));
    let stamp_path = out_root.join("build-stamp.txt");
    if fs::read_to_string(&stamp_path).ok().as_deref() != Some(stamp.as_str()) {
        remove_path_if_exists(&source_copy)?;
        remove_path_if_exists(&build)?;
    }
    fs::create_dir_all(&out_root)?;
    sync_build_source(&source, &source_copy)?;
    // OpenCV intentionally compiles its complete configure report into
    // libopencv_core.  Redact the disposable checkout prefix while the report
    // is assembled; compiler prefix maps cannot affect this CMake-generated
    // string payload.
    let utilities = source_copy.join("cmake/OpenCVUtils.cmake");
    let utilities_body = fs::read_to_string(&utilities)?;
    let marker = "function(ocv_output_status msg)\n  message(STATUS \"${msg}\")";
    let replacement = "function(ocv_output_status msg)\n  if(DEFINED MATTOS_BUILD_ROOT)\n    string(REPLACE \"${MATTOS_BUILD_ROOT}\" \"/usr/src/mattos\" msg \"${msg}\")\n  endif()\n  message(STATUS \"${msg}\")";
    if !utilities_body.contains(marker) {
        bail!("OpenCV build-information adaptation target changed upstream");
    }
    fs::write(&utilities, utilities_body.replacen(marker, replacement, 1))?;
    let core_cmake = source_copy.join("modules/core/CMakeLists.txt");
    let core_cmake_body = fs::read_to_string(&core_cmake)?;
    let build_dir_define = "#define OPENCV_BUILD_DIR \\\"${CMAKE_BINARY_DIR}\\\"";
    if !core_cmake_body.contains(build_dir_define) {
        bail!("OpenCV data-path build-directory definition changed upstream");
    }
    fs::write(
        &core_cmake,
        core_cmake_body.replace(
            build_dir_define,
            "#define OPENCV_BUILD_DIR \\\"/usr/src/mattos/opencv-build\\\"",
        ),
    )?;
    let env = staged_library_environment(repo_root, &["zlib"])?;
    if !build.join("build.ninja").is_file() {
        let mut args = vec![
            "-S".to_string(), source_copy.display().to_string(),
            "-B".to_string(), build.display().to_string(),
            "-G".to_string(), "Ninja".to_string(),
        ];
        args.extend(options.iter().cloned());
        let args = args.iter().map(String::as_str).collect::<Vec<_>>();
        run_cmd_with_env_overrides(repo_root, "cmake", &args, &env)?;
    }
    run_cmd_with_env_overrides(repo_root, "cmake", &["--build", path_str(&build)?], &env)?;
    remove_path_if_exists(&install)?;
    run_cmd_with_env_overrides(
        repo_root,
        "cmake",
        &["--install", path_str(&build)?, "--prefix", path_str(&install.join("usr"))?],
        &env,
    )?;
    for required in [
        "usr/lib/x86_64-linux-gnu/libopencv_core.so",
        "usr/lib/x86_64-linux-gnu/libopencv_imgproc.so",
        "usr/lib/x86_64-linux-gnu/cmake/opencv4/OpenCVConfig.cmake",
    ] {
        if !install.join(required).exists() {
            bail!("OpenCV build did not install /{required}");
        }
    }
    fs::write(stamp_path, stamp)?;
    Ok(())
}

fn build_zxing_cpp(repo_root: &Path) -> Result<()> {
    let source = repo_root.join("src/system/libraries/zxing-cpp");
    let out_root = repo_root.join("out/build/zxing-cpp");
    let source_copy = out_root.join("source");
    let build = out_root.join("build");
    let install = out_root.join("install");
    let state = fs::read_to_string(repo_root.join("upstream/state/zxing-cpp.toml"))?;
    let options = [
        "-DCMAKE_INSTALL_PREFIX=/usr",
        "-DCMAKE_INSTALL_LIBDIR=lib/x86_64-linux-gnu",
        "-DCMAKE_BUILD_TYPE=Release",
        "-DBUILD_SHARED_LIBS=ON",
        "-DZXING_READERS=ON",
        // Prison needs readers and PDF417 generation. The legacy writer is
        // implemented in-tree; the new writer delegates to zint's optional
        // Git submodule, which is intentionally not part of this import.
        "-DZXING_WRITERS=OLD",
        "-DZXING_C_API=OFF",
        "-DZXING_EXAMPLES=OFF",
        "-DZXING_EXAMPLES_QT=OFF",
        "-DZXING_BLACKBOX_TESTS=OFF",
        "-DZXING_UNIT_TESTS=OFF",
        "-DZXING_TEST_INSTALL=OFF",
        "-DZXING_PYTHON_MODULE=OFF",
        "-DZXING_DEPENDENCIES=LOCAL",
    ];
    let stamp = format!("{state}\n{}\n", options.join("\n"));
    let stamp_path = out_root.join("build-stamp.txt");
    if fs::read_to_string(&stamp_path).ok().as_deref() != Some(stamp.as_str()) {
        remove_path_if_exists(&source_copy)?;
        remove_path_if_exists(&build)?;
    }
    fs::create_dir_all(&out_root)?;
    sync_build_source(&source, &source_copy)?;
    let env = staged_library_environment(repo_root, &[])?;
    if !build.join("build.ninja").is_file() {
        let mut args = vec![
            "-S".to_string(), source_copy.display().to_string(),
            "-B".to_string(), build.display().to_string(),
            "-G".to_string(), "Ninja".to_string(),
        ];
        args.extend(options.iter().map(|value| value.to_string()));
        let args = args.iter().map(String::as_str).collect::<Vec<_>>();
        run_cmd_with_env_overrides(repo_root, "cmake", &args, &env)?;
    }
    run_cmd_with_env_overrides(repo_root, "cmake", &["--build", path_str(&build)?], &env)?;
    remove_path_if_exists(&install)?;
    run_cmd_with_env_overrides(
        repo_root,
        "cmake",
        &["--install", path_str(&build)?, "--prefix", path_str(&install.join("usr"))?],
        &env,
    )?;
    for required in [
        "usr/lib/x86_64-linux-gnu/libZXing.so",
        "usr/lib/x86_64-linux-gnu/cmake/ZXing/ZXingConfig.cmake",
    ] {
        if !install.join(required).exists() {
            bail!("ZXing-C++ build did not install /{required}");
        }
    }
    fs::write(stamp_path, stamp)?;
    Ok(())
}

fn build_pulseaudio_client(repo_root: &Path) -> Result<()> {
    build_meson_runtime(
        repo_root,
        "pulseaudio",
        "src/system/multimedia/pulseaudio",
        &["libsndfile", "glib"],
        &[
            "--prefix=/usr",
            "--libdir=lib/x86_64-linux-gnu",
            "--buildtype=release",
            "-Ddaemon=false",
            "-Dclient=true",
            "-Ddoxygen=false",
            "-Dman=false",
            "-Dtests=false",
            "-Dalsa=disabled",
            "-Dasyncns=disabled",
            "-Davahi=disabled",
            "-Dbluez5=disabled",
            "-Dconsolekit=disabled",
            "-Ddbus=disabled",
            "-Delogind=disabled",
            "-Dfftw=disabled",
            "-Dglib=enabled",
            "-Dgsettings=disabled",
            "-Dgstreamer=disabled",
            "-Dgtk=disabled",
            "-Djack=disabled",
            "-Dlirc=disabled",
            "-Dopenssl=disabled",
            "-Dorc=disabled",
            "-Doss-output=disabled",
            "-Dsamplerate=disabled",
            "-Dsoxr=disabled",
            "-Dspeex=disabled",
            "-Dsystemd=disabled",
            "-Dtcpwrap=disabled",
            "-Dudev=disabled",
            "-Dvalgrind=disabled",
            "-Dx11=disabled",
            "-Dadrian-aec=false",
            "-Dwebrtc-aec=disabled",
        ],
        "usr/lib/x86_64-linux-gnu/libpulse.so.0",
        &[],
    )
}

fn build_libsndfile(repo_root: &Path) -> Result<()> {
    let source = repo_root.join("src/system/multimedia/libsndfile");
    let out_root = repo_root.join("out/build/libsndfile");
    let source_copy = out_root.join("source");
    let build = out_root.join("build");
    let install = out_root.join("install");
    let state = fs::read_to_string(repo_root.join("upstream/state/libsndfile.toml"))?;
    let options = [
        "-DCMAKE_INSTALL_PREFIX=/usr",
        "-DCMAKE_INSTALL_LIBDIR=lib/x86_64-linux-gnu",
        "-DCMAKE_BUILD_TYPE=Release",
        "-DCMAKE_POLICY_VERSION_MINIMUM=3.5",
        "-DBUILD_SHARED_LIBS=ON",
        "-DBUILD_PROGRAMS=OFF",
        "-DBUILD_EXAMPLES=OFF",
        "-DBUILD_TESTING=OFF",
        "-DBUILD_REGTEST=OFF",
        "-DENABLE_EXTERNAL_LIBS=OFF",
        "-DENABLE_MPEG=OFF",
        "-DENABLE_EXPERIMENTAL=OFF",
        "-DENABLE_CPACK=OFF",
        "-DENABLE_PACKAGE_CONFIG=ON",
        "-DINSTALL_MANPAGES=OFF",
    ];
    let stamp = format!("{state}\n{}\n", options.join("\n"));
    let stamp_path = out_root.join("build-stamp.txt");
    if fs::read_to_string(&stamp_path).ok().as_deref() != Some(stamp.as_str()) {
        remove_path_if_exists(&source_copy)?;
        remove_path_if_exists(&build)?;
    }
    fs::create_dir_all(&out_root)?;
    sync_build_source(&source, &source_copy)?;
    let env = staged_library_environment(repo_root, &[])?;
    if !build.join("build.ninja").is_file() {
        let mut args = vec![
            "-S".to_string(), source_copy.display().to_string(),
            "-B".to_string(), build.display().to_string(),
            "-G".to_string(), "Ninja".to_string(),
        ];
        args.extend(options.iter().map(|value| value.to_string()));
        let args = args.iter().map(String::as_str).collect::<Vec<_>>();
        run_cmd_with_env_overrides(repo_root, "cmake", &args, &env)?;
    }
    run_cmd_with_env_overrides(repo_root, "cmake", &["--build", path_str(&build)?], &env)?;
    remove_path_if_exists(&install)?;
    run_cmd_with_env_overrides(
        repo_root,
        "cmake",
        &["--install", path_str(&build)?, "--prefix", path_str(&install.join("usr"))?],
        &env,
    )?;
    for required in [
        "usr/lib/x86_64-linux-gnu/libsndfile.so",
        "usr/lib/x86_64-linux-gnu/pkgconfig/sndfile.pc",
    ] {
        if !install.join(required).exists() {
            bail!("libsndfile build did not install /{required}");
        }
    }
    fs::write(stamp_path, stamp)?;
    Ok(())
}

fn build_libgudev(repo_root: &Path) -> Result<()> {
    build_meson_runtime(
        repo_root,
        "libgudev",
        "src/system/libraries/libgudev",
        &["glib", "systemd"],
        &[
            "--prefix=/usr",
            "--libdir=lib/x86_64-linux-gnu",
            "--buildtype=release",
            "-Dtests=disabled",
            "-Dintrospection=disabled",
            "-Dvapi=disabled",
            "-Dgtk_doc=false",
        ],
        "usr/lib/x86_64-linux-gnu/libgudev-1.0.so.0",
        &[],
    )
}

fn build_libbytesize(repo_root: &Path) -> Result<()> {
    build_autotools_import(
        repo_root,
        "libbytesize",
        "src/system/libraries/libbytesize",
        &["pcre2", "gmp", "mpfr"],
        &[
            "--prefix=/usr",
            "--libdir=/usr/lib/x86_64-linux-gnu",
            "--disable-static",
            "--without-python3",
            "--without-tools",
            "--without-gtk-doc",
        ],
        &["usr/lib/x86_64-linux-gnu/libbytesize.so", "usr/include/bytesize/bs_size.h"],
    )?;
    remove_path_if_exists(
        &repo_root.join("out/build/libbytesize/install/usr/lib/x86_64-linux-gnu/libbytesize.la"),
    )?;
    Ok(())
}

fn build_gmp(repo_root: &Path) -> Result<()> {
    build_autotools_import(
        repo_root,
        "gmp",
        "src/system/libraries/gmp",
        &[],
        &[
            "--prefix=/usr",
            "--libdir=/usr/lib/x86_64-linux-gnu",
            "--disable-static",
            "--enable-shared",
            "--enable-cxx",
        ],
        &["usr/lib/x86_64-linux-gnu/libgmp.so", "usr/include/gmp.h"],
    )?;
    // GMP exposes its configure-time CFLAGS through a public macro.  The
    // target compiler flags deliberately contain the absolute MattOS
    // sysroot, which must not leak into a redistributable development
    // header.  This macro is informational; retain the reproducible policy
    // flags while removing the build-machine path.
    let header = repo_root.join("out/build/gmp/install/usr/include/gmp.h");
    let contents = fs::read_to_string(&header)?;
    let normalized = contents
        .lines()
        .map(|line| {
            if line.starts_with("#define __GMP_CFLAGS ") {
                "#define __GMP_CFLAGS \"-O2 -g0\""
            } else {
                line
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    if normalized == contents {
        bail!("GMP public CFLAGS definition was not found for normalization");
    }
    fs::write(&header, normalized)?;
    remove_path_if_exists(&repo_root.join("out/build/gmp/install/usr/lib/x86_64-linux-gnu/libgmp.la"))?;
    remove_path_if_exists(&repo_root.join("out/build/gmp/install/usr/lib/x86_64-linux-gnu/libgmpxx.la"))?;
    Ok(())
}

fn build_mpfr(repo_root: &Path) -> Result<()> {
    build_autotools_import(
        repo_root,
        "mpfr",
        "src/system/libraries/mpfr",
        &["gmp"],
        &[
            "--prefix=/usr",
            "--libdir=/usr/lib/x86_64-linux-gnu",
            "--disable-static",
            "--enable-shared",
        ],
        &["usr/lib/x86_64-linux-gnu/libmpfr.so", "usr/include/mpfr.h"],
    )?;
    remove_path_if_exists(&repo_root.join("out/build/mpfr/install/usr/lib/x86_64-linux-gnu/libmpfr.la"))?;
    Ok(())
}

fn build_keyutils(repo_root: &Path) -> Result<()> {
    let out = repo_root.join("out/build/keyutils");
    let source = repo_root.join("src/system/security/keyutils");
    let source_copy = out.join("source");
    let install = out.join("install");
    let stamp = "keyutils-v1\nlibdir=/usr/lib/x86_64-linux-gnu\n";
    let stamp_path = out.join("build-stamp.txt");
    if fs::read_to_string(&stamp_path).ok().as_deref() != Some(stamp) {
        remove_path_if_exists(&source_copy)?;
        remove_path_if_exists(&install)?;
    }
    fs::create_dir_all(&out)?;
    sync_build_source(&source, &source_copy)?;
    let env = staged_library_environment(repo_root, &[])?;
    let variables = [
        "NO_ARLIB=1",
        "LIBDIR=/usr/lib/x86_64-linux-gnu",
        "USRLIBDIR=/usr/lib/x86_64-linux-gnu",
        "BINDIR=/usr/bin",
        "SBINDIR=/usr/sbin",
    ];
    let mut build_args = vec!["-j", "4"];
    build_args.extend(variables);
    run_cmd_with_env_overrides(&source_copy, "make", &build_args, &env)?;
    remove_path_if_exists(&install)?;
    let destdir = format!("DESTDIR={}", install.display());
    let mut args = vec!["install", destdir.as_str()];
    args.extend(variables);
    run_cmd_with_env_overrides(&source_copy, "make", &args, &env)?;
    for required in [
        "usr/lib/x86_64-linux-gnu/libkeyutils.so.1",
        "usr/include/keyutils.h",
        "usr/lib/x86_64-linux-gnu/pkgconfig/libkeyutils.pc",
    ] {
        if !install.join(required).exists() {
            bail!("keyutils build did not install /{required}");
        }
    }
    fs::write(stamp_path, stamp)?;
    Ok(())
}

fn build_libnvme(repo_root: &Path) -> Result<()> {
    build_meson_runtime(
        repo_root,
        "libnvme",
        "src/system/libraries/libnvme",
        &["openssl", "keyutils"],
        &[
            "--prefix=/usr",
            "--libdir=lib/x86_64-linux-gnu",
            "--buildtype=release",
            "-Ddocs=false",
            "-Ddocs-build=false",
            "-Dexamples=false",
            "-Dtests=false",
            "-Dpython=disabled",
            "-Dopenssl=enabled",
            "-Djson-c=disabled",
            "-Dkeyutils=enabled",
            "-Dlibdbus=disabled",
            "-Dliburing=disabled",
        ],
        "usr/lib/x86_64-linux-gnu/libnvme.so",
        &[],
    )
}

fn build_popt(repo_root: &Path) -> Result<()> {
    build_autotools_import(
        repo_root,
        "popt",
        "src/system/libraries/popt",
        &[],
        &["--prefix=/usr", "--libdir=/usr/lib/x86_64-linux-gnu", "--disable-static"],
        &["usr/lib/x86_64-linux-gnu/libpopt.so", "usr/include/popt.h"],
    )?;
    remove_staged_libtool_archives(
        &repo_root.join("out/build/popt/install/usr/lib/x86_64-linux-gnu"),
    )?;
    Ok(())
}

fn build_cmake_runtime(
    repo_root: &Path,
    component: &str,
    source_relative: &str,
    dependencies: &[&str],
    options: &[&str],
    required_output: &str,
) -> Result<()> {
    let out = repo_root.join("out/build").join(component);
    let source = repo_root.join(source_relative);
    let source_copy = out.join("source");
    let build = out.join("build");
    let install = out.join("install");
    let stamp = format!("cmake-runtime-v1\n{}\ndependencies={}\n", options.join("\n"), dependencies.join(","));
    let stamp_path = out.join("build-stamp.txt");
    if fs::read_to_string(&stamp_path).ok().as_deref() != Some(stamp.as_str()) {
        remove_path_if_exists(&source_copy)?;
        remove_path_if_exists(&build)?;
    }
    fs::create_dir_all(&out)?;
    sync_build_source(&source, &source_copy)?;
    let env = staged_library_environment(repo_root, dependencies)?;
    if !build.join("build.ninja").is_file() {
        let mut args = vec![
            "-S".to_string(), source_copy.display().to_string(),
            "-B".to_string(), build.display().to_string(),
            "-G".to_string(), "Ninja".to_string(),
        ];
        args.extend(options.iter().map(|value| (*value).to_string()));
        run_cmd_with_env_overrides(repo_root, "cmake", &args.iter().map(String::as_str).collect::<Vec<_>>(), &env)?;
    }
    run_cmd_with_env_overrides(repo_root, "cmake", &["--build", path_str(&build)?, "--", "-j", "4"], &env)?;
    remove_path_if_exists(&install)?;
    let mut install_env = env.clone();
    install_env.push(("DESTDIR", install.display().to_string()));
    run_cmd_with_env_overrides(
        repo_root,
        "cmake",
        &["--install", path_str(&build)?],
        &install_env,
    )?;
    if !install.join(required_output).exists() {
        bail!("{component} build did not install /{required_output}");
    }
    fs::write(stamp_path, stamp)?;
    Ok(())
}

fn build_json_c(repo_root: &Path) -> Result<()> {
    build_cmake_runtime(
        repo_root,
        "json-c",
        "src/system/libraries/json-c",
        &[],
        &[
            "-DCMAKE_INSTALL_PREFIX=/usr",
            "-DCMAKE_INSTALL_LIBDIR=lib/x86_64-linux-gnu",
            "-DBUILD_SHARED_LIBS=ON",
            "-DBUILD_STATIC_LIBS=OFF",
            "-DBUILD_TESTING=OFF",
            "-DDISABLE_WERROR=ON",
        ],
        "usr/lib/x86_64-linux-gnu/libjson-c.so",
    )
}

fn build_e2fsprogs(repo_root: &Path) -> Result<()> {
    build_autotools_import(
        repo_root,
        "e2fsprogs",
        "src/system/storage/e2fsprogs",
        &["util-linux"],
        &[
            "--prefix=/usr",
            "--sbindir=/usr/sbin",
            "--libdir=/usr/lib/x86_64-linux-gnu",
            "--sysconfdir=/etc",
            "--enable-elf-shlibs",
            "--disable-nls",
            "--disable-uuidd",
            "--disable-fuse2fs",
            "--disable-fsck",
        ],
        &[
            "usr/sbin/mkfs.ext4",
            "usr/lib/x86_64-linux-gnu/libext2fs.so",
            "usr/lib/x86_64-linux-gnu/pkgconfig/ext2fs.pc",
            "usr/lib/x86_64-linux-gnu/pkgconfig/e2p.pc",
        ],
    )
}

fn build_device_mapper(repo_root: &Path) -> Result<()> {
    build_autotools_import(
        repo_root,
        "lvm2",
        "src/system/storage/lvm2",
        &["util-linux"],
        &[
            "--prefix=/usr",
            "--libdir=/usr/lib/x86_64-linux-gnu",
            "--sbindir=/usr/sbin",
            "--disable-static_link",
            "--disable-lvm1_fallback",
            "--disable-readline",
            "--disable-selinux",
            "--disable-udev_sync",
            "--disable-udev_rules",
            "--disable-thin_check_needs_check",
            "--disable-cache_check_needs_check",
            "--disable-vdo",
            "--disable-dmeventd",
            "--disable-cmdlib",
            "--disable-python_bindings",
            "--enable-pkgconfig",
        ],
        &[
            "usr/lib/x86_64-linux-gnu/libdevmapper.so.1.02",
            "usr/include/libdevmapper.h",
        ],
    )?;

    // LVM's device-mapper-only install places the unversioned development
    // symlink and pkg-config metadata in /usr/lib even when its shared object
    // is correctly installed in the target multiarch directory.  Normalize
    // those development artifacts so downstream target-only discovery cannot
    // fall back to the host linker search path.
    let install = repo_root.join("out/build/lvm2/install/usr/lib");
    let multiarch = install.join("x86_64-linux-gnu");
    let unversioned = multiarch.join("libdevmapper.so");
    remove_path_if_exists(&unversioned)?;
    #[cfg(unix)]
    std::os::unix::fs::symlink("libdevmapper.so.1.02", &unversioned)?;
    remove_path_if_exists(&install.join("libdevmapper.so"))?;
    let pkgconfig = multiarch.join("pkgconfig");
    fs::create_dir_all(&pkgconfig)?;
    let source_pc = install.join("pkgconfig/devmapper.pc");
    if source_pc.is_file() {
        fs::rename(&source_pc, pkgconfig.join("devmapper.pc"))?;
    }
    Ok(())
}

fn build_cryptsetup(repo_root: &Path) -> Result<()> {
    build_autotools_import(
        repo_root,
        "cryptsetup",
        "src/system/storage/cryptsetup",
        &[
            "openssl",
            "keyutils",
            "util-linux",
            "popt",
            "json-c",
            "lvm2",
            "zlib",
            "zstd",
        ],
        &[
            "--prefix=/usr",
            "--libdir=/usr/lib/x86_64-linux-gnu",
            "--disable-static",
            "--disable-asciidoc",
            "--disable-ssh-token",
            "--disable-external-tokens",
            "--disable-pwquality",
            "--disable-selinux",
            "--disable-udev",
            "--disable-kernel_crypto",
            "--disable-internal-argon2",
            "--disable-blkid",
            "--disable-hw-opal",
            "--with-crypto_backend=openssl",
        ],
        &["usr/lib/x86_64-linux-gnu/libcryptsetup.so"],
    )?;
    remove_staged_libtool_archives(
        &repo_root.join("out/build/cryptsetup/install/usr/lib/x86_64-linux-gnu"),
    )?;
    Ok(())
}

fn build_libblockdev(repo_root: &Path) -> Result<()> {
    build_autotools_import(
        repo_root,
        "libblockdev",
        "src/system/libraries/libblockdev",
        &[
            "glib", "systemd", "kmod", "util-linux", "pcre2", "libffi",
            "zlib", "zstd", "xz", "openssl", "libbytesize", "keyutils",
            "libnvme", "cryptsetup", "e2fsprogs", "lvm2", "json-c", "selinux",
        ],
        &[
            "--prefix=/usr",
            "--libdir=/usr/lib/x86_64-linux-gnu",
            "--disable-static",
            "--disable-tests",
            "--without-python3",
            "--without-gtk-doc",
            "--without-tools",
            "--without-escrow",
            "--without-btrfs",
            "--without-dm",
            "--without-lvm",
            "--without-lvm-dbus",
            "--without-mpath",
            "--without-nvdimm",
            "--without-smart",
            "--without-smartmontools",
        ],
        &[
            "usr/lib/x86_64-linux-gnu/libblockdev.so",
            "usr/lib/x86_64-linux-gnu/libbd_part.so",
            "usr/lib/x86_64-linux-gnu/libbd_loop.so",
            "usr/lib/x86_64-linux-gnu/libbd_swap.so",
            "usr/lib/x86_64-linux-gnu/libbd_mdraid.so",
            "usr/lib/x86_64-linux-gnu/libbd_fs.so",
            "usr/lib/x86_64-linux-gnu/libbd_crypto.so",
            "usr/lib/x86_64-linux-gnu/libbd_nvme.so",
        ],
    )?;
    // Libtool archives retain build-time absolute dependency paths and can
    // make later consumers ignore the corresponding shared objects. They are
    // not runtime or supported development metadata.
    remove_staged_libtool_archives(
        &repo_root.join("out/build/libblockdev/install/usr/lib/x86_64-linux-gnu"),
    )?;
    Ok(())
}

fn build_wireplumber(repo_root: &Path) -> Result<()> {
    build_meson_runtime(
        repo_root,
        "wireplumber",
        "src/system/multimedia/wireplumber",
        &["glib", "pipewire", "systemd", "dbus", "pcre2", "libffi", "zlib"],
        &[
            "--prefix=/usr",
            "--libdir=lib/x86_64-linux-gnu",
            "--buildtype=release",
            "-Dintrospection=disabled",
            "-Ddoc=disabled",
            "-Dmodules=true",
            "-Ddaemon=true",
            "-Dtools=true",
            "-Dsystem-lua=false",
            "-Delogind=disabled",
            "-Dsystemd=enabled",
            "-Dsystemd-system-service=false",
            "-Dsystemd-user-service=true",
            "-Dtests=false",
            "-Ddbus-tests=false",
        ],
        "usr/bin/wireplumber",
        &[],
    )
}

fn build_upower(repo_root: &Path) -> Result<()> {
    build_meson_runtime(
        repo_root,
        "upower",
        "src/system/services/upower",
        &["glib", "libgudev", "polkit", "systemd", "dbus", "pcre2", "libffi", "zlib"],
        &[
            "--prefix=/usr",
            "--libdir=lib/x86_64-linux-gnu",
            "--buildtype=release",
            "-Dman=false",
            "-Dgtk-doc=false",
            "-Dintrospection=disabled",
            "-Didevice=disabled",
            "-Dpolkit=enabled",
            "-Dinstalled_tests=false",
            "-Dos_backend=linux",
        ],
        "usr/libexec/upowerd",
        &[],
    )
}

fn build_power_profiles_daemon(repo_root: &Path) -> Result<()> {
    build_meson_runtime(
        repo_root,
        "power-profiles-daemon",
        "src/system/services/power-profiles-daemon",
        &["glib", "libgudev", "upower", "polkit", "systemd", "dbus", "pcre2", "libffi", "zlib"],
        &[
            "--prefix=/usr",
            "--libdir=lib/x86_64-linux-gnu",
            "--buildtype=release",
            "-Dgtk_doc=false",
            "-Dpylint=disabled",
            "-Dtests=false",
            "-Dmanpage=disabled",
            "-Dbashcomp=disabled",
            "-Dzshcomp=",
        ],
        "usr/libexec/power-profiles-daemon",
        &[],
    )
}

fn build_udisks2(repo_root: &Path) -> Result<()> {
    build_autotools_import(
        repo_root,
        "udisks2",
        "src/system/services/udisks2",
        &["glib", "libgudev", "libblockdev", "polkit", "systemd", "util-linux", "dbus", "pcre2", "libffi", "zlib", "zstd", "xz", "openssl", "kmod", "selinux"],
        &[
            "--prefix=/usr",
            "--libdir=/usr/lib/x86_64-linux-gnu",
            "--disable-static",
            "--disable-man",
            "--disable-gtk-doc",
            "--disable-introspection",
            "--disable-acl",
            "--disable-lvm2",
            "--disable-iscsi",
            "--disable-btrfs",
            "--disable-lsm",
            "--disable-smart",
            "--disable-modules",
            "--disable-available-modules",
        ],
        &["usr/libexec/udisks2/udisksd", "usr/bin/udisksctl"],
    )?;
    remove_staged_libtool_archives(
        &repo_root.join("out/build/udisks2/install/usr/lib/x86_64-linux-gnu"),
    )?;
    Ok(())
}

fn build_bluez(repo_root: &Path) -> Result<()> {
    build_autotools_import(
        repo_root,
        "bluez",
        "src/system/services/bluez",
        &["glib", "dbus", "systemd", "pcre2", "libffi", "zlib", "readline", "ncurses"],
        &[
            "--prefix=/usr",
            "--libdir=/usr/lib/x86_64-linux-gnu",
            "--enable-library",
            "--enable-client",
            "--enable-systemd",
            "--enable-udev",
            "--with-dbusconfdir=/usr/share",
            "--with-dbussystembusdir=/usr/share/dbus-1/system-services",
            "--with-udevdir=/usr/lib/udev",
            "--with-systemdsystemunitdir=/usr/lib/systemd/system",
            "--with-systemduserunitdir=/usr/lib/systemd/user",
            "--disable-static",
            "--disable-test",
            "--disable-testing",
            "--disable-tools",
            "--disable-monitor",
            "--disable-cups",
            "--disable-mesh",
            "--disable-midi",
            "--disable-obex",
            "--disable-btpclient",
            "--disable-manpages",
        ],
        &["usr/libexec/bluetooth/bluetoothd", "usr/bin/bluetoothctl"],
    )?;
    remove_staged_libtool_archives(
        &repo_root.join("out/build/bluez/install/usr/lib/x86_64-linux-gnu"),
    )?;
    Ok(())
}


fn build_pipewire(repo_root: &Path) -> Result<()> {
    build_meson_runtime(
        repo_root,
        "pipewire",
        "src/system/multimedia/pipewire",
        &["systemd", "dbus", "openssl"],
        &[
            "--prefix=/usr",
            "--libdir=lib/x86_64-linux-gnu",
            "--buildtype=release",
            "-Ddocs=disabled",
            "-Dman=disabled",
            "-Dexamples=disabled",
            "-Dtests=disabled",
            "-Dinstalled_tests=disabled",
            "-Dgstreamer=disabled",
            "-Dsystemd=enabled",
            "-Dlogind=enabled",
            "-Dsystemd-system-service=disabled",
            "-Dsystemd-user-service=enabled",
            "-Dselinux=disabled",
            "-Dpipewire-alsa=disabled",
            "-Dpipewire-jack=disabled",
            "-Dpipewire-v4l2=disabled",
            "-Dalsa=disabled",
            "-Dbluez5=disabled",
            "-Dffmpeg=disabled",
            "-Djack=disabled",
            "-Dv4l2=disabled",
            "-Dlibcamera=disabled",
            "-Dvulkan=disabled",
            "-Dsdl2=disabled",
            "-Dsndfile=disabled",
            "-Dlibmysofa=disabled",
            "-Dlibpulse=disabled",
            "-Davahi=disabled",
            "-Dlibusb=disabled",
            "-Dsession-managers=[]",
            "-Dx11=disabled",
            "-Dx11-xfixes=disabled",
            "-Dlibcanberra=disabled",
            "-Dlegacy-rtkit=false",
            "-Dflatpak=disabled",
            "-Dreadline=disabled",
            "-Dgsettings=disabled",
            "-Dgsettings-pulse-schema=disabled",
        ],
        "usr/lib/x86_64-linux-gnu/libpipewire-0.3.so.0",
        &[],
    )?;
    let pipewire_usr = repo_root.join("out/build/pipewire/install/usr");
    rewrite_pkgconfig_prefixes(
        &pipewire_usr.join("lib/x86_64-linux-gnu/pkgconfig"),
        &pipewire_usr,
    )?;
    for required in [
        "usr/bin/pipewire",
        "usr/bin/pipewire-pulse",
        "usr/lib/systemd/user/pipewire.service",
        "usr/lib/systemd/user/pipewire.socket",
    ] {
        if !repo_root
            .join("out/build/pipewire/install")
            .join(required)
            .exists()
        {
            bail!("PipeWire build did not install /{required}");
        }
    }
    Ok(())
}
