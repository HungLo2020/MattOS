use anyhow::Result;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
#[cfg(test)]
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Component, Path, PathBuf};
#[cfg(not(test))]
use std::sync::Mutex;

const SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
pub(crate) struct FileFingerprint {
    device: u64,
    inode: u64,
    file_type: u32,
    size: u64,
    mtime_seconds: i64,
    mtime_nanoseconds: i64,
    ctime_seconds: i64,
    ctime_nanoseconds: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
struct PersistentFileDigest {
    fingerprint: FileFingerprint,
    sha256: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct PersistentIntegrityIndexFile {
    schema_version: u32,
    entries_sha256: String,
    entries: BTreeMap<String, PersistentFileDigest>,
}

struct PersistentIntegrityIndex {
    repo_root: PathBuf,
    entries: BTreeMap<String, PersistentFileDigest>,
    dirty: bool,
}

#[cfg(not(test))]
static INDEX: Mutex<Option<PersistentIntegrityIndex>> = Mutex::new(None);
#[cfg(test)]
thread_local! {
    static INDEX: RefCell<Option<PersistentIntegrityIndex>> = const { RefCell::new(None) };
}

#[cfg(not(test))]
fn with_index<R>(action: impl FnOnce(&mut Option<PersistentIntegrityIndex>) -> R) -> R {
    let mut index = INDEX.lock().expect("integrity index mutex poisoned");
    action(&mut index)
}

#[cfg(test)]
fn with_index<R>(action: impl FnOnce(&mut Option<PersistentIntegrityIndex>) -> R) -> R {
    INDEX.with(|slot| action(&mut slot.borrow_mut()))
}

pub(crate) fn start(repo_root: &Path) {
    let path = path(repo_root);
    let entries = fs::read(&path)
        .ok()
        .and_then(|body| serde_json::from_slice::<PersistentIntegrityIndexFile>(&body).ok())
        .filter(|index| index.schema_version == SCHEMA_VERSION)
        .filter(|index| {
            digest_serializable(&(index.schema_version, &index.entries))
                .is_ok_and(|digest| digest == index.entries_sha256)
        })
        .filter(|index| entries_valid(&index.entries))
        .map(|index| index.entries)
        .unwrap_or_default();
    with_index(|index| *index = Some(PersistentIntegrityIndex {
        repo_root: repo_root.to_path_buf(),
        entries,
        dirty: false,
    }));
}

pub(crate) fn clear() {
    with_index(|index| *index = None);
}

pub(crate) fn serialized_if_dirty() -> Result<Option<(PathBuf, Vec<u8>)>> {
    with_index(|borrowed| {
        let Some(index) = borrowed.as_ref().filter(|index| index.dirty) else {
            return Ok(None);
        };
        let file = PersistentIntegrityIndexFile {
            schema_version: SCHEMA_VERSION,
            entries_sha256: digest_serializable(&(SCHEMA_VERSION, &index.entries))?,
            entries: index.entries.clone(),
        };
        Ok(Some((
            path(&index.repo_root),
            serde_json::to_vec_pretty(&file)?,
        )))
    })
}

pub(crate) fn path(repo_root: &Path) -> PathBuf {
    repo_root.join("out/state/integrity-index.json")
}

pub(crate) fn eligible(path: &Path) -> bool {
    with_index(|index| index.as_ref().is_some_and(|index| key(index, path).is_some()))
}

pub(crate) fn lookup(path: &Path, fingerprint: &FileFingerprint) -> Option<String> {
    with_index(|borrowed| {
        let index = borrowed.as_mut()?;
        let key = key(index, path)?;
        let digest = index
            .entries
            .get(&key)
            .filter(|entry| entry.fingerprint == *fingerprint)
            .map(|entry| entry.sha256.clone());
        if digest.is_none() && index.entries.remove(&key).is_some() {
            index.dirty = true;
        }
        digest
    })
}

pub(crate) fn store(path: &Path, fingerprint: FileFingerprint, sha256: String) {
    with_index(|borrowed| {
        let Some(index) = borrowed.as_mut() else {
            return;
        };
        let Some(key) = key(index, path) else {
            return;
        };
        index.entries.insert(
            key,
            PersistentFileDigest {
                fingerprint,
                sha256,
            },
        );
        index.dirty = true;
    });
}

pub(crate) fn invalidate(paths: &[PathBuf]) {
    with_index(|borrowed| {
        let Some(index) = borrowed.as_mut() else {
            return;
        };
        let original_len = index.entries.len();
        index.entries.retain(|path, _| {
            let absolute = index.repo_root.join(path);
            !paths
                .iter()
                .any(|changed| paths_overlap(&absolute, changed))
        });
        index.dirty |= index.entries.len() != original_len;
    });
}

#[cfg(unix)]
pub(crate) fn fingerprint(metadata: &fs::Metadata) -> Option<FileFingerprint> {
    use std::os::unix::fs::MetadataExt;

    Some(FileFingerprint {
        device: metadata.dev(),
        inode: metadata.ino(),
        file_type: metadata.mode() & 0o170000,
        size: metadata.size(),
        mtime_seconds: metadata.mtime(),
        mtime_nanoseconds: metadata.mtime_nsec(),
        ctime_seconds: metadata.ctime(),
        ctime_nanoseconds: metadata.ctime_nsec(),
    })
}

#[cfg(not(unix))]
pub(crate) fn fingerprint(_metadata: &fs::Metadata) -> Option<FileFingerprint> {
    None
}

fn key(index: &PersistentIntegrityIndex, path: &Path) -> Option<String> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir().ok()?.join(path)
    };
    if !absolute.starts_with(index.repo_root.join("out")) {
        return None;
    }
    Some(normalize_path(
        absolute.strip_prefix(&index.repo_root).ok()?,
    ))
}

fn entries_valid(entries: &BTreeMap<String, PersistentFileDigest>) -> bool {
    entries.iter().all(|(path, entry)| {
        let path = Path::new(path);
        !path.is_absolute()
            && !path
                .components()
                .any(|component| matches!(component, Component::ParentDir))
            && path.starts_with("out")
            && entry.fingerprint.file_type == 0o100000
            && entry.sha256.len() == 64
            && entry.sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
    })
}

fn paths_overlap(left: &Path, right: &Path) -> bool {
    left.starts_with(right) || right.starts_with(left)
}

fn digest_serializable<T: Serialize + ?Sized>(value: &T) -> Result<String> {
    let body = serde_json::to_vec(value)?;
    Ok(format!("{:x}", Sha256::digest(body)))
}

fn normalize_path(path: &Path) -> String {
    path.components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}
