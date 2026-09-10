// Runtime Wi-Fi closure. Authoritative source is never built in place.
fn build_libnl(repo_root: &Path) -> Result<()> {
    build_autotools_import(repo_root, "libnl", "src/system/network/libnl", &[],
        &["--prefix=/usr", "--libdir=/usr/lib/x86_64-linux-gnu", "--disable-static", "--disable-cli"],
        &["usr/lib/x86_64-linux-gnu/libnl-3.so.200", "usr/lib/x86_64-linux-gnu/libnl-genl-3.so.200"])
}

fn build_wpa_supplicant(repo_root: &Path) -> Result<()> {
    let out = repo_root.join("out/build/wpa-supplicant");
    let source = out.join("source");
    // Invoked only after authoritative stage-cache evaluation; discard private
    // objects on a real miss, not published dependency outputs.
    remove_path_if_exists(&source)?;
    sync_build_source(&repo_root.join("src/system/network/hostap"), &source)?;
    let work = source.join("wpa_supplicant");
    fs::copy(repo_root.join("src/system/network/wpa-supplicant/build.config"), work.join(".config"))?;
    let mut env = staged_library_environment(repo_root, &["openssl", "libnl", "dbus"])?;
    // Upstream consumes CFLAGS, not CPPFLAGS, for these external headers.
    let includes = env.iter().find(|(key, _)| *key == "CPPFLAGS").unwrap().1.clone();
    env.push(("CFLAGS", format!("-O2 {includes}")));
    run_cmd_with_env_overrides(&work, "make", &["-j", "4", "wpa_supplicant", "wpa_cli", "wpa_passphrase"], &env)?;
    let install = out.join("install");
    remove_path_if_exists(&install)?;
    fs::create_dir_all(install.join("usr/sbin"))?;
    for name in ["wpa_supplicant", "wpa_cli", "wpa_passphrase"] {
        fs::copy(work.join(name), install.join("usr/sbin").join(name))?;
        set_mode(install.join("usr/sbin").join(name), 0o755)?;
    }
    fs::create_dir_all(install.join("usr/share/dbus-1/system-services"))?;
    fs::create_dir_all(install.join("usr/share/dbus-1/system.d"))?;
    let activation = fs::read_to_string(work.join("dbus/fi.w1.wpa_supplicant1.service.in"))?;
    fs::write(install.join("usr/share/dbus-1/system-services/fi.w1.wpa_supplicant1.service"), activation.replace("@BINDIR@", "/usr/sbin"))?;
    fs::copy(work.join("dbus/dbus-wpa_supplicant.conf"), install.join("usr/share/dbus-1/system.d/wpa_supplicant.conf"))?;
    fs::create_dir_all(install.join("usr/lib/systemd/system"))?;
    fs::copy(repo_root.join("src/system/network/wpa-supplicant/wpa_supplicant.service"), install.join("usr/lib/systemd/system/wpa_supplicant.service"))?;
    Ok(())
}

#[cfg(test)]
mod wifi_tests {
    use super::*;

    #[test]
    fn wifi_backend_has_owned_runtime_and_dbus_activation_closure() {
        let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
        let config = fs::read_to_string(repo.join("src/system/network/wpa-supplicant/build.config")).unwrap();
        for flag in ["CONFIG_DRIVER_NL80211=y", "CONFIG_CTRL_IFACE_DBUS_NEW=y", "CONFIG_LIBNL32=y", "CONFIG_TLS=openssl", "CONFIG_SAE=y"] {
            assert!(config.lines().any(|line| line == flag));
        }
        let service = fs::read_to_string(repo.join("src/system/network/wpa-supplicant/wpa_supplicant.service")).unwrap();
        assert!(service.contains("BusName=fi.w1.wpa_supplicant1"));
        assert!(service.contains("ExecStart=/usr/sbin/wpa_supplicant -u -s"));
        assert!(!service.contains("-i wlan"));
        let activation = fs::read_to_string(repo.join("src/system/network/hostap/wpa_supplicant/dbus/fi.w1.wpa_supplicant1.service.in")).unwrap();
        assert!(activation.contains("SystemdService=wpa_supplicant.service"));
    }

    #[test]
    fn wifi_recipe_inputs_and_edges_are_explicit() {
        assert!(stage_inputs::source_inputs(BuildStage::WpaSupplicant).contains(&PathBuf::from("src/system/network/wpa-supplicant")));
        for dependency in ["libnl", "openssl", "dbus"] {
            assert!(stage_graph::direct_dependencies(BuildStage::WpaSupplicant).contains(&dependency));
        }
        assert!(!stage_inputs::source_inputs(BuildStage::Mesa).contains(&PathBuf::from("src/tools/mattos-build/src/stages/wifi.rs")));
        assert!(!stage_graph::direct_dependencies(BuildStage::NetworkManager).contains(&"wpa-supplicant"));
    }
}
