use super::*;

pub(crate) fn generate_repository(repo_root: &Path) -> Result<()> {
    let spec = repository_stage_spec(repo_root)?;
    performance::execute_cached_stage(
        repo_root,
        &spec,
        || {
            validate_repository_against_inventory(
                &repo_root.join("out/repository"),
                &read_inventory(repo_root)?,
            )
        },
        || generate_repository_atomic(repo_root),
    )
}

pub(crate) fn repository_stage_spec(repo_root: &Path) -> Result<performance::StageSpec> {
    // inventory.toml is the ordered package name/version/architecture/SHA set.
    // Do not hash every .deb again merely to compute the key; package manifests
    // already validate those artifacts and a miss validates every copied pool
    // object against the recorded SHA before publication.
    let _ = read_inventory(repo_root)?;
    let inputs = vec![PathBuf::from("out/packages/inventory.toml")];
    Ok(performance::StageSpec {
        id: "repository".into(),
        source_inputs: Vec::new(),
        configuration_inputs: inputs,
        tools: vec![
            "dpkg-scanpackages".into(),
            "apt-ftparchive".into(),
            "gzip".into(),
        ],
        dependencies: Vec::new(),
        outputs: vec!["out/repository".into()],
        recipe: format!(
            "repository-v2:suite=trixie:codename=trixie:component=main:arch={ARCH}:origin=MattOS:label=MattOS Local:gzip=-n,-9:epoch={SOURCE_DATE_EPOCH}:manifest-schema={}",
            performance::STAGE_MANIFEST_SCHEMA_VERSION
        ),
    })
}

pub(super) fn generate_repository_atomic(repo_root: &Path) -> Result<()> {
    let repository = repo_root.join("out/repository");
    let temp = performance::temporary_sibling(&repository, "building")?;
    let result = generate_repository_inner(repo_root, &temp);
    if let Err(error) = result {
        let _ = remove_path_if_exists(&temp);
        return Err(error);
    }
    performance::atomic_replace_path(&temp, &repository)
}

pub(super) fn generate_repository_inner(repo_root: &Path, repository: &Path) -> Result<()> {
    let inventory = read_inventory(repo_root)?;
    for name in PACKAGE_NAMES {
        if !inventory.package.iter().any(|entry| entry.name == *name) {
            bail!("package {name} has not been built");
        }
    }
    let mut inventory_keys = BTreeSet::new();
    for entry in &inventory.package {
        if !PACKAGE_NAMES.contains(&entry.name.as_str()) {
            bail!("inventory contains unexpected package {}", entry.name)
        }
        let key = (&entry.name, &entry.version, &entry.architecture);
        if !inventory_keys.insert(key) {
            bail!(
                "duplicate package/version/architecture in inventory: {} {} {}",
                entry.name,
                entry.version,
                entry.architecture
            )
        }
    }
    let pool = repository.join("pool/main");
    let index_dir = repository.join("dists/trixie/main/binary-amd64");
    fs::create_dir_all(&pool)?;
    fs::create_dir_all(&index_dir)?;
    for entry in &inventory.package {
        let source = repo_root.join(&entry.artifact_path);
        let file_name = source
            .file_name()
            .ok_or_else(|| anyhow!("invalid artifact path"))?;
        fs::copy(&source, pool.join(file_name))?;
    }
    let scan = Command::new("dpkg-scanpackages")
        .args(["pool/main", "/dev/null"])
        .current_dir(&repository)
        .output()
        .context("failed to run dpkg-scanpackages")?;
    if !scan.status.success() {
        bail!(
            "dpkg-scanpackages failed: {}",
            String::from_utf8_lossy(&scan.stderr)
        );
    }
    let packages = index_dir.join("Packages");
    fs::write(&packages, scan.stdout)?;
    let gzip = Command::new("gzip")
        .args(["-n", "-9", "-c", path_str(&packages)?])
        .output()?;
    if !gzip.status.success() {
        bail!("gzip failed for Packages index");
    }
    fs::write(index_dir.join("Packages.gz"), gzip.stdout)?;

    let release = Command::new("apt-ftparchive")
        .args([
            "-o",
            "APT::FTPArchive::Release::Origin=MattOS",
            "-o",
            "APT::FTPArchive::Release::Label=MattOS Local",
            "-o",
            "APT::FTPArchive::Release::Suite=trixie",
            "-o",
            "APT::FTPArchive::Release::Codename=trixie",
            "-o",
            "APT::FTPArchive::Release::Architectures=amd64",
            "-o",
            "APT::FTPArchive::Release::Components=main",
            "-o",
            "APT::FTPArchive::Release::Description=Local MattOS bootstrap repository",
            "release",
            "dists/trixie",
        ])
        .env("SOURCE_DATE_EPOCH", SOURCE_DATE_EPOCH.to_string())
        .current_dir(&repository)
        .output()
        .context("failed to run apt-ftparchive")?;
    if !release.status.success() {
        bail!(
            "apt-ftparchive failed: {}",
            String::from_utf8_lossy(&release.stderr)
        );
    }
    let release_body = String::from_utf8(release.stdout)?;
    let release_body = release_body
        .lines()
        .map(|line| {
            if line.starts_with("Date: ") {
                "Date: Thu, 01 Jan 2026 00:00:00 +0000"
            } else {
                line
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    fs::write(repository.join("dists/trixie/Release"), release_body)?;
    validate_repository_against_inventory(repository, &inventory)?;
    println!(
        "generated local MattOS repository at {}",
        repository.display()
    );
    Ok(())
}

pub(super) fn validate_repository(repository: &Path) -> Result<()> {
    let packages = fs::read_to_string(repository.join("dists/trixie/main/binary-amd64/Packages"))?;
    if packages.contains("deb.debian.org") || packages.contains("archive.ubuntu.com") {
        bail!("foreign repository URL found in Packages");
    }
    validate_repository_packages(&packages)?;
    let release = fs::read_to_string(repository.join("dists/trixie/Release"))?;
    for field in [
        "Origin: MattOS",
        "Label: MattOS Local",
        "Suite: trixie",
        "Codename: trixie",
        "Architectures: amd64",
        "Components: main",
        "SHA256:",
    ] {
        if !release.contains(field) {
            bail!("Release missing {field}");
        }
    }
    Ok(())
}

pub(super) fn validate_repository_against_inventory(
    repository: &Path,
    inventory: &PackageInventory,
) -> Result<()> {
    validate_repository(repository)?;
    let packages_path = repository.join("dists/trixie/main/binary-amd64/Packages");
    let packages_body = fs::read_to_string(&packages_path)?;
    let paragraphs = parse_control_paragraphs(&packages_body)?;
    if paragraphs.len() != inventory.package.len() {
        bail!("Packages entry count differs from package inventory");
    }
    let mut expected_files = BTreeSet::new();
    for entry in &inventory.package {
        let artifact_name = Path::new(&entry.artifact_path)
            .file_name()
            .and_then(OsStr::to_str)
            .ok_or_else(|| anyhow!("invalid package artifact path {}", entry.artifact_path))?;
        let relative = format!("pool/main/{artifact_name}");
        expected_files.insert(relative.clone());
        let pool_artifact = repository.join(&relative);
        if sha256_file(&pool_artifact)? != entry.sha256 {
            bail!("repository artifact digest differs for {}", entry.name);
        }
        let paragraph = paragraphs
            .iter()
            .find(|paragraph| paragraph.get("Package") == Some(&entry.name))
            .ok_or_else(|| anyhow!("Packages index missing {}", entry.name))?;
        if control_field(paragraph, "Version")? != entry.version
            || control_field(paragraph, "Architecture")? != entry.architecture
            || control_field(paragraph, "Filename")? != relative
            || control_field(paragraph, "SHA256")? != entry.sha256
        {
            bail!(
                "Packages metadata differs from inventory for {}",
                entry.name
            );
        }
    }
    let pool = repository.join("pool/main");
    let actual_files = fs::read_dir(&pool)?
        .map(|entry| {
            entry.map(|entry| format!("pool/main/{}", entry.file_name().to_string_lossy()))
        })
        .collect::<std::io::Result<BTreeSet<_>>>()?;
    if actual_files != expected_files {
        bail!("repository pool path set differs from package inventory");
    }

    let compressed = Command::new("gzip")
        .args([
            "-dc",
            path_str(&packages_path.with_file_name("Packages.gz"))?,
        ])
        .output()?;
    if !compressed.status.success() || compressed.stdout != packages_body.as_bytes() {
        bail!("Packages.gz is corrupt or differs from Packages");
    }
    validate_release_sha256(repository)?;
    Ok(())
}

pub(super) fn validate_release_sha256(repository: &Path) -> Result<()> {
    let release_path = repository.join("dists/trixie/Release");
    let body = fs::read_to_string(&release_path)?;
    let mut in_sha256 = false;
    let mut checked = BTreeSet::new();
    for line in body.lines() {
        if line == "SHA256:" {
            in_sha256 = true;
            continue;
        }
        if in_sha256 && !line.starts_with(' ') {
            break;
        }
        if !in_sha256 || line.trim().is_empty() {
            continue;
        }
        let fields = line.split_whitespace().collect::<Vec<_>>();
        if fields.len() != 3 {
            bail!("invalid SHA256 entry in Release: {line}");
        }
        let relative = fields[2];
        let path = release_path.parent().unwrap().join(relative);
        let metadata = fs::metadata(&path)?;
        if metadata.len().to_string() != fields[1] || sha256_file(&path)? != fields[0] {
            bail!("Release SHA256 mismatch for {relative}");
        }
        checked.insert(relative.to_string());
    }
    for required in [
        "main/binary-amd64/Packages",
        "main/binary-amd64/Packages.gz",
    ] {
        if !checked.contains(required) {
            bail!("Release SHA256 inventory missing {required}");
        }
    }
    Ok(())
}

pub(super) fn validate_repository_packages(body: &str) -> Result<()> {
    package_install_order()?;
    let paragraphs = parse_control_paragraphs(body)?;
    let mut by_name: BTreeMap<String, Vec<(String, String)>> = BTreeMap::new();
    let mut provided = BTreeSet::new();
    let mut keys = BTreeSet::new();
    for paragraph in &paragraphs {
        let name = control_field(paragraph, "Package")?.to_string();
        let version = control_field(paragraph, "Version")?.to_string();
        let architecture = control_field(paragraph, "Architecture")?.to_string();
        validate_package_name(&name)?;
        validate_debian_version(&version)?;
        if architecture != ARCH {
            bail!("repository package {name} has architecture {architecture}, expected {ARCH}")
        }
        if !keys.insert((name.clone(), version.clone(), architecture.clone())) {
            bail!(
                "duplicate repository package/version/architecture: {name} {version} {architecture}"
            )
        }
        by_name
            .entry(name)
            .or_default()
            .push((version, architecture));
        if let Some(provides) = paragraph.get("Provides") {
            for item in provides.split(',') {
                let virtual_name = dependency_name(item)?;
                provided.insert(virtual_name.to_string());
            }
        }
    }
    for name in PACKAGE_NAMES {
        if !by_name.contains_key(*name) {
            bail!("Packages index missing {name}")
        }
    }
    for paragraph in &paragraphs {
        let package = control_field(paragraph, "Package")?;
        let Some(depends) = paragraph.get("Depends") else {
            continue;
        };
        for group in depends
            .split(',')
            .map(str::trim)
            .filter(|group| !group.is_empty())
        {
            let mut satisfied = false;
            for alternative in group.split('|').map(str::trim) {
                let name = dependency_name(alternative)?;
                if let Some(candidates) = by_name.get(name) {
                    if let Some(expected) = exact_dependency_version(alternative)? {
                        satisfied = candidates.iter().any(|(version, _)| version == expected);
                    } else {
                        satisfied = true;
                    }
                } else if provided.contains(name)
                    && exact_dependency_version(alternative)?.is_none()
                {
                    satisfied = true;
                }
                if satisfied {
                    break;
                }
            }
            if !satisfied {
                bail!("repository dependency for {package} is unsatisfied: {group}")
            }
        }
    }
    Ok(())
}

pub(super) fn parse_control_paragraphs(body: &str) -> Result<Vec<BTreeMap<String, String>>> {
    let mut paragraphs = Vec::new();
    for raw in body
        .split("\n\n")
        .filter(|paragraph| !paragraph.trim().is_empty())
    {
        let mut paragraph: BTreeMap<String, String> = BTreeMap::new();
        let mut last_key: Option<String> = None;
        for line in raw.lines() {
            if line.starts_with([' ', '\t']) {
                if let Some(key) = &last_key {
                    paragraph.get_mut(key).expect("field exists").push_str(line);
                }
                continue;
            }
            let (key, value) = line
                .split_once(':')
                .ok_or_else(|| anyhow!("invalid control line {line:?}"))?;
            paragraph.insert(key.to_string(), value.trim().to_string());
            last_key = Some(key.to_string());
        }
        paragraphs.push(paragraph);
    }
    Ok(paragraphs)
}

pub(super) fn control_field<'a>(paragraph: &'a BTreeMap<String, String>, field: &str) -> Result<&'a str> {
    paragraph
        .get(field)
        .map(String::as_str)
        .ok_or_else(|| anyhow!("repository package paragraph lacks {field}"))
}

pub(super) fn dependency_name(relation: &str) -> Result<&str> {
    let name = relation
        .trim()
        .split([' ', '('])
        .next()
        .unwrap_or_default()
        .split(':')
        .next()
        .unwrap_or_default();
    validate_package_name(name)?;
    Ok(name)
}

pub(super) fn exact_dependency_version(relation: &str) -> Result<Option<&str>> {
    let Some((_, constraint)) = relation.split_once('(') else {
        return Ok(None);
    };
    let constraint = constraint.trim_end_matches(')').trim();
    let mut fields = constraint.split_whitespace();
    let operator = fields.next().unwrap_or_default();
    let version = fields.next().unwrap_or_default();
    if operator != "=" || version.is_empty() {
        bail!("MattOS bootstrap repository only accepts exact dependency constraints: {relation}")
    }
    Ok(Some(version))
}
