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
const BINARY_MAGIC: &[u8] = b"MATTOS-INTEGRITY\0";

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
        .and_then(|body| parse_binary(&body).or_else(|| parse_legacy_json(&body)))
        .filter(entries_valid)
        .unwrap_or_default();
    with_index(|index| {
        *index = Some(PersistentIntegrityIndex {
            repo_root: repo_root.to_path_buf(),
            entries,
            dirty: false,
        })
    });
}

pub(crate) fn clear() {
    with_index(|index| *index = None);
}

pub(crate) fn serialized_if_dirty() -> Result<Option<(PathBuf, Vec<u8>)>> {
    with_index(|borrowed| {
        let Some(index) = borrowed.as_mut() else {
            return Ok(None);
        };
        // Never persist digests for files that no longer exist (deleted stage
        // workspaces, replaced outputs); otherwise the index only ever grows.
        let repo_root = index.repo_root.clone();
        let original_len = index.entries.len();
        index.entries.retain(|path, _| {
            fs::symlink_metadata(repo_root.join(path)).is_ok_and(|metadata| metadata.is_file())
        });
        index.dirty |= index.entries.len() != original_len;
        if !index.dirty {
            return Ok(None);
        }
        Ok(Some((
            path(&index.repo_root),
            serialize_binary(&index.entries),
        )))
    })
}

fn parse_legacy_json(body: &[u8]) -> Option<BTreeMap<String, PersistentFileDigest>> {
    serde_json::from_slice::<PersistentIntegrityIndexFile>(body)
        .ok()
        .filter(|index| index.schema_version == SCHEMA_VERSION)
        .filter(|index| {
            digest_serializable(&(index.schema_version, &index.entries))
                .is_ok_and(|digest| digest == index.entries_sha256)
        })
        .map(|index| index.entries)
}

fn put_u32(body: &mut Vec<u8>, value: u32) {
    body.extend_from_slice(&value.to_le_bytes());
}

fn put_u64(body: &mut Vec<u8>, value: u64) {
    body.extend_from_slice(&value.to_le_bytes());
}

fn put_i64(body: &mut Vec<u8>, value: i64) {
    body.extend_from_slice(&value.to_le_bytes());
}

fn take<'a>(body: &'a [u8], offset: &mut usize, length: usize) -> Option<&'a [u8]> {
    let end = offset.checked_add(length)?;
    let value = body.get(*offset..end)?;
    *offset = end;
    Some(value)
}

fn take_u32(body: &[u8], offset: &mut usize) -> Option<u32> {
    Some(u32::from_le_bytes(take(body, offset, 4)?.try_into().ok()?))
}

fn take_u64(body: &[u8], offset: &mut usize) -> Option<u64> {
    Some(u64::from_le_bytes(take(body, offset, 8)?.try_into().ok()?))
}

fn take_i64(body: &[u8], offset: &mut usize) -> Option<i64> {
    Some(i64::from_le_bytes(take(body, offset, 8)?.try_into().ok()?))
}

fn serialize_binary(entries: &BTreeMap<String, PersistentFileDigest>) -> Vec<u8> {
    let mut body = Vec::new();
    body.extend_from_slice(BINARY_MAGIC);
    put_u32(&mut body, SCHEMA_VERSION);
    put_u64(&mut body, entries.len() as u64);
    for (path, entry) in entries {
        let path = path.as_bytes();
        put_u32(&mut body, path.len() as u32);
        body.extend_from_slice(path);
        put_u64(&mut body, entry.fingerprint.device);
        put_u64(&mut body, entry.fingerprint.inode);
        put_u32(&mut body, entry.fingerprint.file_type);
        put_u64(&mut body, entry.fingerprint.size);
        put_i64(&mut body, entry.fingerprint.mtime_seconds);
        put_i64(&mut body, entry.fingerprint.mtime_nanoseconds);
        put_i64(&mut body, entry.fingerprint.ctime_seconds);
        put_i64(&mut body, entry.fingerprint.ctime_nanoseconds);
        put_u32(&mut body, entry.sha256.len() as u32);
        body.extend_from_slice(entry.sha256.as_bytes());
    }
    let checksum = Sha256::digest(&body);
    body.extend_from_slice(&checksum);
    body
}

fn parse_binary(body: &[u8]) -> Option<BTreeMap<String, PersistentFileDigest>> {
    let checksum_offset = body.len().checked_sub(32)?;
    let (payload, checksum) = body.split_at(checksum_offset);
    if Sha256::digest(payload).as_slice() != checksum {
        return None;
    }
    let mut offset = 0;
    if take(payload, &mut offset, BINARY_MAGIC.len())? != BINARY_MAGIC {
        return None;
    }
    if take_u32(payload, &mut offset)? != SCHEMA_VERSION {
        return None;
    }
    let count = take_u64(payload, &mut offset)? as usize;
    let mut entries = BTreeMap::new();
    for _ in 0..count {
        let path_length = take_u32(payload, &mut offset)? as usize;
        let path = String::from_utf8(take(payload, &mut offset, path_length)?.to_vec()).ok()?;
        let fingerprint = FileFingerprint {
            device: take_u64(payload, &mut offset)?,
            inode: take_u64(payload, &mut offset)?,
            file_type: take_u32(payload, &mut offset)?,
            size: take_u64(payload, &mut offset)?,
            mtime_seconds: take_i64(payload, &mut offset)?,
            mtime_nanoseconds: take_i64(payload, &mut offset)?,
            ctime_seconds: take_i64(payload, &mut offset)?,
            ctime_nanoseconds: take_i64(payload, &mut offset)?,
        };
        let digest_length = take_u32(payload, &mut offset)? as usize;
        let sha256 = String::from_utf8(take(payload, &mut offset, digest_length)?.to_vec()).ok()?;
        if entries
            .insert(
                path,
                PersistentFileDigest {
                    fingerprint,
                    sha256,
                },
            )
            .is_some()
        {
            return None;
        }
    }
    (offset == payload.len()).then_some(entries)
}

pub(crate) fn path(repo_root: &Path) -> PathBuf {
    repo_root.join("out/state/integrity-index.json")
}

pub(crate) fn eligible(path: &Path) -> bool {
    with_index(|index| {
        index
            .as_ref()
            .is_some_and(|index| key(index, path).is_some())
    })
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
    let relative = absolute.strip_prefix(&index.repo_root).ok()?;
    // Files under a transient build directory are renamed or deleted within
    // the run that created them; caching them only accumulates dead entries.
    if relative.components().any(|component| {
        component
            .as_os_str()
            .to_str()
            .is_some_and(is_transient_directory_name)
    }) {
        return None;
    }
    Some(normalize_path(relative))
}

/// `performance::temporary_sibling` and `atomic_replace_path` name transient
/// directories `.<name>.<label>-<pid>` (e.g. `.rootfs.building-1234`,
/// `.rootfs.previous-1234`).
fn is_transient_directory_name(name: &str) -> bool {
    let Some(rest) = name.strip_prefix('.') else {
        return false;
    };
    let Some((head, pid)) = rest.rsplit_once('-') else {
        return false;
    };
    !pid.is_empty() && pid.bytes().all(|byte| byte.is_ascii_digit()) && head.contains('.')
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

#[cfg(test)]
mod tests {
    use super::*;

    fn fingerprint_for(path: &Path) -> FileFingerprint {
        super::fingerprint(&fs::metadata(path).unwrap()).unwrap()
    }

    #[test]
    fn transient_build_directories_are_never_indexed() {
        let temporary = tempfile::tempdir().unwrap();
        let repo = temporary.path();
        start(repo);
        for (relative, expected) in [
            ("out/build/.rootfs.building-1234/usr/bin/true", false),
            ("out/build/.rootfs.previous-99/usr/bin/true", false),
            ("out/build/rootfs/usr/bin/true", true),
            ("out/build/.pkgconfig-overlays/abc/lib/x.pc", true),
        ] {
            assert_eq!(eligible(&repo.join(relative)), expected, "{relative}");
        }
        clear();
    }

    #[test]
    fn saving_prunes_digests_of_files_that_no_longer_exist() {
        let temporary = tempfile::tempdir().unwrap();
        let repo = temporary.path();
        let kept = repo.join("out/build/a/kept");
        let removed = repo.join("out/build/b/removed");
        for file in [&kept, &removed] {
            fs::create_dir_all(file.parent().unwrap()).unwrap();
            fs::write(file, "payload").unwrap();
        }
        start(repo);
        store(&kept, fingerprint_for(&kept), "a".repeat(64));
        store(&removed, fingerprint_for(&removed), "b".repeat(64));
        fs::remove_dir_all(repo.join("out/build/b")).unwrap();
        let (_, body) = serialized_if_dirty().unwrap().unwrap();
        let entries = parse_binary(&body).unwrap();
        assert_eq!(entries.keys().collect::<Vec<_>>(), ["out/build/a/kept"]);
        clear();
    }
}
