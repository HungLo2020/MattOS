use crate::performance;
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

const ELF_FACT_SCHEMA: u32 = 1;
const ELF_POLICY: &str = "mattos-amd64-runtime-v1";

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
pub(crate) struct ElfFacts {
    pub(crate) schema_version: u32,
    pub(crate) content_sha256: String,
    pub(crate) target_architecture: String,
    pub(crate) validation_policy: String,
    pub(crate) readelf_version: String,
    pub(crate) elf_type: String,
    pub(crate) architecture: String,
    pub(crate) interpreter: Option<String>,
    pub(crate) soname: Option<String>,
    pub(crate) needed: Vec<String>,
    pub(crate) rpath: Vec<String>,
    pub(crate) runpath: Vec<String>,
    pub(crate) symbol_versions: Vec<String>,
    pub(crate) build_id: Option<String>,
}

pub(crate) fn inspect(repo_root: &Path, path: &Path) -> Result<Option<ElfFacts>> {
    let mut file = fs::File::open(path)?;
    let mut magic = [0u8; 4];
    if file.read_exact(&mut magic).is_err() || magic != *b"\x7fELF" {
        return Ok(None);
    }
    let digest = sha256_file(path)?;
    let version = readelf_version()?;
    let cache = cache_path(repo_root, &digest);
    if let Ok(body) = fs::read(&cache) {
        if let Ok(facts) = serde_json::from_slice::<ElfFacts>(&body) {
            if facts.schema_version == ELF_FACT_SCHEMA
                && facts.content_sha256 == digest
                && facts.target_architecture == "x86_64-linux-gnu"
                && facts.validation_policy == ELF_POLICY
                && facts.readelf_version == version
            {
                return Ok(Some(facts));
            }
        }
    }

    let output = Command::new("readelf")
        .args(["-h", "-l", "-d", "-V", "-n"])
        .arg(path)
        .output()
        .with_context(|| format!("failed to inspect {} with readelf", path.display()))?;
    if !output.status.success() {
        bail!("readelf rejected ELF object {}", path.display());
    }
    let text = String::from_utf8(output.stdout)?;
    let facts = parse_facts(&digest, &version, &text);
    performance::atomic_write_json(&cache, &facts)?;
    Ok(Some(facts))
}

fn parse_facts(digest: &str, version: &str, text: &str) -> ElfFacts {
    let field = |label: &str| {
        text.lines()
            .find_map(|line| line.trim().strip_prefix(label).map(str::trim))
            .unwrap_or_default()
            .to_string()
    };
    let bracket_values = |marker: &str| {
        text.lines()
            .filter(|line| line.contains(marker))
            .filter_map(|line| line.split('[').nth(1)?.split(']').next())
            .map(str::to_string)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>()
    };
    let interpreter = text.lines().find_map(|line| {
        line.split_once("Requesting program interpreter:")
            .map(|(_, value)| value.trim().trim_end_matches(']').to_string())
    });
    let symbol_versions = text
        .split(|character: char| {
            character.is_whitespace() || matches!(character, '(' | ')' | '[' | ']')
        })
        .filter(|word| {
            ["GLIBC_", "GLIBCXX_", "CXXABI_", "GCC_"]
                .iter()
                .any(|prefix| word.starts_with(prefix))
        })
        .map(|word| {
            word.trim_matches(|character: char| {
                !character.is_ascii_alphanumeric() && character != '_' && character != '.'
            })
            .to_string()
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let build_id = text.lines().find_map(|line| {
        line.trim()
            .strip_prefix("Build ID:")
            .map(|value| value.trim().to_string())
    });
    ElfFacts {
        schema_version: ELF_FACT_SCHEMA,
        content_sha256: digest.to_string(),
        target_architecture: "x86_64-linux-gnu".into(),
        validation_policy: ELF_POLICY.into(),
        readelf_version: version.to_string(),
        elf_type: field("Type:"),
        architecture: field("Machine:"),
        interpreter,
        soname: bracket_values("(SONAME)").into_iter().next(),
        needed: bracket_values("(NEEDED)"),
        rpath: bracket_values("(RPATH)"),
        runpath: bracket_values("(RUNPATH)"),
        symbol_versions,
        build_id,
    }
}

fn cache_path(repo_root: &Path, digest: &str) -> PathBuf {
    repo_root
        .join("out/state/elf")
        .join(format!("{digest}.json"))
}

fn readelf_version() -> Result<String> {
    static VERSION: OnceLock<String> = OnceLock::new();
    if let Some(version) = VERSION.get() {
        return Ok(version.clone());
    }
    let output = Command::new("readelf").arg("--version").output()?;
    if !output.status.success() {
        bail!("readelf --version failed");
    }
    let version = String::from_utf8_lossy(&output.stdout)
        .lines()
        .next()
        .unwrap_or_default()
        .to_string();
    let _ = VERSION.set(version.clone());
    Ok(version)
}

fn sha256_file(path: &Path) -> Result<String> {
    let mut file = fs::File::open(path)?;
    let mut digest = Sha256::new();
    let mut buffer = [0u8; 1024 * 1024];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
    }
    Ok(format!("{:x}", digest.finalize()))
}

pub(crate) fn invalidate(repo_root: &Path) -> Result<usize> {
    let root = repo_root.join("out/state/elf");
    if !root.is_dir() {
        return Ok(0);
    }
    let count = fs::read_dir(&root)?.count();
    fs::remove_dir_all(root)?;
    Ok(count)
}

pub(crate) fn status(repo_root: &Path) -> Result<String> {
    let root = repo_root.join("out/state/elf");
    let count = if root.is_dir() {
        fs::read_dir(root)?.count()
    } else {
        0
    };
    Ok(format!(
        "elf-facts: {count} content-addressed inspection record(s); schema={ELF_FACT_SCHEMA}"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parsing_is_path_independent_and_stable() {
        let text = "Type: DYN\nMachine: Advanced Micro Devices X86-64\n[Requesting program interpreter: /lib64/ld-linux-x86-64.so.2]\n0 (NEEDED) Shared library: [libc.so.6]\n0 (SONAME) Library soname: [libdemo.so.1]\nBuild ID: abc\nGLIBC_2.38\n";
        let first = parse_facts("abc", "readelf 1", text);
        let second = parse_facts("abc", "readelf 1", text);
        assert_eq!(first, second);
        assert_eq!(first.needed, ["libc.so.6"]);
        assert_eq!(first.soname.as_deref(), Some("libdemo.so.1"));
    }

    #[test]
    fn identical_elf_content_at_two_paths_reuses_one_fact_record() {
        let root = tempfile::tempdir().unwrap();
        let first_path = root.path().join("first");
        let second_path = root.path().join("second");
        fs::copy("/bin/true", &first_path).unwrap();
        fs::copy("/bin/true", &second_path).unwrap();
        let first = inspect(root.path(), &first_path).unwrap().unwrap();
        let second = inspect(root.path(), &second_path).unwrap().unwrap();
        assert_eq!(first, second);
        assert_eq!(
            fs::read_dir(root.path().join("out/state/elf"))
                .unwrap()
                .count(),
            1
        );
    }
}
