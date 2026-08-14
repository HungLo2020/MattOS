//! Offline discovery of user-selectable regional settings.
//!
//! The installer deliberately derives these choices from the same files that
//! are shipped in the live image.  The identifiers in this module are the
//! identifiers stored in [`crate::InstallPlan`]; labels are presentation only.

use crate::policy::LIVE_SOURCE;
use anyhow::{Context, Result, bail};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct Choice {
    pub id: String,
    pub label: String,
}

impl Choice {
    fn new(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
        }
    }
}

impl fmt::Display for Choice {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.label)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KeyboardLayout {
    pub id: String,
    pub label: String,
    /// Valid variants for this layout. The empty identifier is the XKB
    /// default and is always the first entry.
    pub variants: Vec<Choice>,
}

impl fmt::Display for KeyboardLayout {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.label)
    }
}

fn runtime_path(relative: &str) -> PathBuf {
    let live = Path::new(LIVE_SOURCE).join(relative);
    if live.exists() {
        live
    } else {
        Path::new("/").join(relative)
    }
}

pub fn discover_locales() -> Result<Vec<Choice>> {
    discover_locales_in(&runtime_path("usr/share/i18n/locales"))
}

pub fn discover_keyboard_layouts() -> Result<Vec<KeyboardLayout>> {
    let candidates = [
        runtime_path("usr/share/X11/xkb/rules/evdev.lst"),
        runtime_path("usr/share/xkeyboard-config-2/rules/evdev.lst"),
    ];
    let path = candidates
        .iter()
        .find(|path| path.is_file())
        .context("MattOS XKB rules do not contain evdev.lst")?;
    discover_keyboard_layouts_in(path)
}

pub fn discover_timezones() -> Result<Vec<Choice>> {
    discover_timezones_in(&runtime_path("usr/share/zoneinfo"))
}

fn quoted_field(source: &str, field: &str) -> Option<String> {
    source.lines().find_map(|line| {
        let rest = line.trim_start().strip_prefix(field)?;
        if !rest.starts_with(char::is_whitespace) {
            return None;
        }
        let value = rest.trim();
        value
            .strip_prefix('"')
            .and_then(|value| value.strip_suffix('"'))
            .map(str::to_owned)
    })
}

fn locale_label(source: &str, fallback: &str) -> Option<String> {
    let language = quoted_field(source, "title")
        .and_then(|title| title.split(" locale").next().map(str::to_owned))
        .filter(|title| !title.is_empty())
        .or_else(|| quoted_field(source, "language"))?;
    let territory = quoted_field(source, "territory");
    Some(match territory.filter(|territory| !territory.is_empty()) {
        Some(territory) => format!("{language} ({territory})"),
        None if language.is_empty() => fallback.replace('_', " "),
        None => language,
    })
}

fn discover_locales_in(directory: &Path) -> Result<Vec<Choice>> {
    let entries = fs::read_dir(directory)
        .with_context(|| format!("read MattOS locale sources at {}", directory.display()))?;
    let mut locales = Vec::new();
    for entry in entries {
        let entry = entry?;
        if !entry.file_type()?.is_file() {
            continue;
        }
        let id = entry.file_name().to_string_lossy().into_owned();
        let source = fs::read_to_string(entry.path())?;
        if let Some(label) = locale_label(&source, &id) {
            locales.push(Choice::new(format!("{id}.UTF-8"), label));
        }
    }
    locales.sort_by(|left, right| left.label.cmp(&right.label).then(left.id.cmp(&right.id)));
    locales.dedup_by(|left, right| left.id == right.id);
    if locales.is_empty() {
        bail!("MattOS locale sources contain no selectable UTF-8 locales");
    }
    Ok(locales)
}

fn discover_keyboard_layouts_in(path: &Path) -> Result<Vec<KeyboardLayout>> {
    let data = fs::read_to_string(path)
        .with_context(|| format!("read MattOS XKB rules at {}", path.display()))?;
    let mut section = "";
    let mut layouts = BTreeMap::<String, String>::new();
    let mut variants = BTreeMap::<String, Vec<Choice>>::new();
    for line in data.lines() {
        let line = line.trim();
        if let Some(name) = line.strip_prefix('!') {
            section = name.trim();
            continue;
        }
        if line.is_empty() {
            continue;
        }
        match section {
            "layout" => {
                if let Some((id, label)) = line.split_once(char::is_whitespace) {
                    layouts.insert(id.to_owned(), label.trim().to_owned());
                }
            }
            "variant" => {
                let mut fields = line.split_whitespace();
                let Some(id) = fields.next() else { continue };
                let Some(layout) = fields.next().and_then(|value| value.strip_suffix(':')) else {
                    continue;
                };
                let label = fields.collect::<Vec<_>>().join(" ");
                variants
                    .entry(layout.to_owned())
                    .or_default()
                    .push(Choice::new(id, label));
            }
            _ => {}
        }
    }
    let mut result = layouts
        .into_iter()
        .map(|(id, label)| {
            let mut choices = vec![Choice::new("", "Default")];
            choices.extend(variants.remove(&id).unwrap_or_default());
            KeyboardLayout {
                id,
                label,
                variants: choices,
            }
        })
        .collect::<Vec<_>>();
    result.sort_by(|left, right| left.label.cmp(&right.label).then(left.id.cmp(&right.id)));
    if result.is_empty() {
        bail!("MattOS XKB rules contain no layouts");
    }
    Ok(result)
}

fn timezone_label(id: &str) -> String {
    let city = id
        .rsplit('/')
        .next()
        .unwrap_or(id)
        .replace('_', " ")
        .replace('-', " ");
    format!("{city} — {id}")
}

fn discover_timezones_in(directory: &Path) -> Result<Vec<Choice>> {
    let mut ids = BTreeSet::new();
    collect_zoneinfo(directory, directory, &mut ids)?;
    let mut result = ids
        .into_iter()
        .map(|id| Choice::new(id.clone(), timezone_label(&id)))
        .collect::<Vec<_>>();
    result.sort_by(|left, right| left.label.cmp(&right.label).then(left.id.cmp(&right.id)));
    if result.is_empty() {
        bail!("MattOS zoneinfo contains no selectable timezones");
    }
    Ok(result)
}

fn collect_zoneinfo(directory: &Path, current: &Path, ids: &mut BTreeSet<String>) -> Result<()> {
    for entry in fs::read_dir(current)? {
        let entry = entry?;
        let path = entry.path();
        let relative = path
            .strip_prefix(directory)
            .expect("zoneinfo traversal remains below its root");
        let first = relative
            .components()
            .next()
            .and_then(|part| part.as_os_str().to_str());
        if matches!(first, Some("posix" | "right")) {
            continue;
        }
        if entry.file_type()?.is_dir() {
            collect_zoneinfo(directory, &path, ids)?;
            continue;
        }
        if matches!(
            relative.to_string_lossy().as_ref(),
            "localtime" | "posixrules"
        ) {
            continue;
        }
        let mut magic = [0_u8; 4];
        if File::open(&path)
            .and_then(|mut file| file.read_exact(&mut magic))
            .is_ok()
            && magic == *b"TZif"
        {
            ids.insert(relative.to_string_lossy().into_owned());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn locale_discovery_maps_source_metadata_to_friendly_labels() {
        let temporary = tempdir().unwrap();
        fs::write(
            temporary.path().join("en_US"),
            "LC_IDENTIFICATION\ntitle \"English locale for the USA\"\nlanguage \"American English\"\nterritory \"United States\"\nEND LC_IDENTIFICATION\n",
        )
        .unwrap();
        fs::write(temporary.path().join("i18n"), "LC_CTYPE\n").unwrap();
        assert_eq!(
            discover_locales_in(temporary.path()).unwrap(),
            vec![Choice::new("en_US.UTF-8", "English (United States)")]
        );
    }

    #[test]
    fn xkb_discovery_filters_variants_by_layout_and_adds_default() {
        let temporary = tempdir().unwrap();
        let rules = temporary.path().join("evdev.lst");
        fs::write(
            &rules,
            "! layout\n  us English (US)\n  de German\n! variant\n  intl us: English (US, intl.)\n  nodeadkeys de: German (no dead keys)\n",
        )
        .unwrap();
        let layouts = discover_keyboard_layouts_in(&rules).unwrap();
        let us = layouts.iter().find(|layout| layout.id == "us").unwrap();
        assert_eq!(us.label, "English (US)");
        assert_eq!(
            us.variants,
            vec![
                Choice::new("", "Default"),
                Choice::new("intl", "English (US, intl.)")
            ]
        );
        assert!(!us.variants.iter().any(|variant| variant.id == "nodeadkeys"));
    }

    #[test]
    fn timezone_discovery_uses_compiled_files_and_friendly_city_names() {
        let temporary = tempdir().unwrap();
        fs::create_dir_all(temporary.path().join("America")).unwrap();
        fs::create_dir_all(temporary.path().join("Etc")).unwrap();
        fs::write(temporary.path().join("America/Los_Angeles"), "TZif").unwrap();
        fs::write(temporary.path().join("Etc/UTC"), "TZif").unwrap();
        fs::write(temporary.path().join("UTC"), "TZif").unwrap();
        fs::write(
            temporary.path().join("zone1970.tab"),
            "US\t+340308-1181434\tAmerica/Los_Angeles\nUS\t+1+1\tMissing/Zone\n",
        )
        .unwrap();
        let zones = discover_timezones_in(temporary.path()).unwrap();
        assert!(zones.contains(&Choice::new(
            "America/Los_Angeles",
            "Los Angeles — America/Los_Angeles"
        )));
        assert!(zones.contains(&Choice::new("Etc/UTC", "UTC — Etc/UTC")));
        assert!(zones.contains(&Choice::new("UTC", "UTC — UTC")));
        assert!(!zones.iter().any(|zone| zone.id == "Missing/Zone"));
    }
}
