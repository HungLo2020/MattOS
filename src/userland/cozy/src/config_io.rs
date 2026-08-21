use crate::state::Config;
use std::io;
use std::path::{Path, PathBuf};

pub const DEFAULT_CONFIG_TOML: &str = r##"# cozy config

page_size = 20
theme = "dark"
show_line_numbers = true
status_duration = 3

line_number_bg = "darkgray"
line_number_fg = "white"

footer_bg = "#222226"
footer_key_fg = "cyan"
footer_fg = "gray"

status_bar_bg = "darkgray"
status_bar_fg = "white"

cursor_blink = true

# Resting mode after actions: "edit" or "glide".
default_mode = "edit"

# Override only the shortcuts you want to change.
# [keys]
# enter_exit = "ctrl+x"
# toggle_markdown = "f2"
"##;

pub fn load() -> Config {
    load_from(None)
}

pub fn load_from(config_dir: Option<&PathBuf>) -> Config {
    let paths = candidate_paths(config_dir);

    if let Some(path) = default_config_path(config_dir) {
        if !path.exists() {
            if let Err(e) = write_default_config(&path) {
                eprintln!(
                    "warning: Failed to create default config '{}': {}",
                    path.display(),
                    e
                );
            }
        }
    }

    for path in &paths {
        if path.exists() {
            match load_from_path(path) {
                Ok(config) => return config,
                Err(e) => {
                    eprintln!(
                        "warning: Failed to parse config file '{}': {}",
                        path.display(),
                        e
                    );
                    eprintln!("warning: Using default configuration");
                }
            }
        }
    }

    Config::default_values()
}

pub fn user_config_path(config_dir: Option<&PathBuf>) -> Option<PathBuf> {
    if let Some(dir) = config_dir {
        return Some(dir.join("config.toml"));
    }
    dirs::config_dir()
        .map(|p| p.join("cozy/config.toml"))
        .or_else(|| dirs::home_dir().map(|p| p.join(".cozy/config.toml")))
}

pub fn load_from_path(path: &Path) -> io::Result<Config> {
    let content = std::fs::read_to_string(path)?;
    toml::from_str::<Config>(&content)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))
}

pub fn ensure_default_config_file(config_dir: Option<&PathBuf>) -> io::Result<PathBuf> {
    let path = default_config_path(config_dir).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "Could not resolve config directory",
        )
    })?;
    if !path.exists() {
        write_default_config(&path)?;
    }
    Ok(path)
}

fn candidate_paths(config_dir: Option<&PathBuf>) -> Vec<PathBuf> {
    if let Some(dir) = config_dir {
        return vec![dir.join("cozy.toml"), dir.join("config.toml")];
    }

    vec![
        dirs::config_dir()
            .map(|p| p.join("cozy/config.toml"))
            .unwrap_or_default(),
        PathBuf::from("config.toml"),
        dirs::home_dir()
            .map(|p| p.join(".cozy/config.toml"))
            .unwrap_or_default(),
    ]
}

fn default_config_path(config_dir: Option<&PathBuf>) -> Option<PathBuf> {
    user_config_path(config_dir)
}

fn write_default_config(path: &Path) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    std::fs::write(path, DEFAULT_CONFIG_TOML)
}
