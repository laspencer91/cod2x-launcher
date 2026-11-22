use crate::github::{self, Release as GitHubRelease};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::HashSet,
    fs::{self, File},
    io::{self, Cursor, Read},
    path::{Path, PathBuf},
};
use zip::ZipArchive;

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

pub const STABLE: &str = "1.4.6.8";
pub const OFFICIAL_ID: &str = "official-cod2x";
const API: &str = "https://api.github.com/repos/callofduty2x/CoD2x/releases";
const ORIGINAL: &str = "7855b8fbae917cb8449f2d4361ab61b5ecec4df0a11130d797cb0aa99b4260ea";
const LOADER: &str = "b994ae0ee542490bb16f28a217c4914da7fdea413232d4e43d1e28ff46917cac";
pub(crate) const MAX_PACKAGE: u64 = 512 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FileSpec {
    pub path: String,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    #[serde(default)]
    pub authors: Vec<String>,
    #[serde(default)]
    pub release_notes: String,
    pub kind: String,
    #[serde(default)]
    pub base_version: String,
    pub files: Vec<FileSpec>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Release {
    pub version: String,
    pub title: String,
    pub notes: String,
    pub published_at: String,
    pub download_size: u64,
    pub sha256: String,
    pub url: String,
}

pub fn releases() -> Result<Vec<Release>> {
    let client = github::client()?;
    let latest: GitHubRelease = github::document(&client, &format!("{API}/latest"), 1024 * 1024)?;
    let latest = release_from_github(latest)
        .ok_or("The latest official release has no verifiable Windows ZIP.")?;
    let mut releases = vec![latest];
    if let Ok(rows) = github::document::<Vec<GitHubRelease>>(
        &client,
        &format!("{API}?per_page=25"),
        4 * 1024 * 1024,
    ) {
        let version = releases[0].version.clone();
        releases.extend(
            rows.into_iter()
                .filter_map(release_from_github)
                .filter(|row| row.version != version),
        );
    }
    Ok(releases)
}

fn release_from_github(row: GitHubRelease) -> Option<Release> {
    let version = row.tag_name.strip_prefix('v')?.to_owned();
    if row.draft || row.prerelease || !official_version(&version) {
        return None;
    }
    let asset = row.assets.into_iter().find(|asset| {
        asset.name == format!("CoD2x_{version}_windows.zip")
            && asset.browser_download_url
                == format!(
                    "https://github.com/callofduty2x/CoD2x/releases/download/v{version}/{}",
                    asset.name
                )
    })?;
    let sha256 = asset.sha256()?;
    if asset.size == 0 || asset.size > MAX_PACKAGE {
        return None;
    }
    Some(Release {
        title: row.name.unwrap_or_else(|| format!("CoD2x {version}")),
        notes: row.body.unwrap_or_default(),
        published_at: row.published_at.unwrap_or_default(),
        download_size: asset.size,
        sha256,
        url: asset.browser_download_url,
        version,
    })
}

pub fn official_version(version: &str) -> bool {
    let parts: Vec<_> = version.split('.').collect();
    parts.len() == 4
        && parts.iter().all(|part| {
            !part.is_empty()
                && part.bytes().all(|b| b.is_ascii_digit())
                && part.parse::<u32>().is_ok()
        })
}

pub(crate) fn package_key(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value.as_bytes()[0].is_ascii_alphanumeric()
        && value.as_bytes()[value.len() - 1].is_ascii_alphanumeric()
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b".-_".contains(&b))
        && !value.contains("..")
        && !matches!(
            value
                .split('.')
                .next()
                .unwrap_or_default()
                .to_ascii_lowercase()
                .as_str(),
            "con"
                | "prn"
                | "aux"
                | "nul"
                | "com0"
                | "com1"
                | "com2"
                | "com3"
                | "com4"
                | "com5"
                | "com6"
                | "com7"
                | "com8"
                | "com9"
                | "lpt0"
                | "lpt1"
                | "lpt2"
                | "lpt3"
                | "lpt4"
                | "lpt5"
                | "lpt6"
                | "lpt7"
                | "lpt8"
                | "lpt9"
        )
}

pub fn valid_hash(hash: &str) -> bool {
    hash.len() == 64
        && hash
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

pub fn iwd_path(path: &str) -> bool {
    path.strip_prefix("main/")
        .and_then(|name| name.strip_suffix(".iwd"))
        .is_some_and(|name| {
            package_key(name) && name == name.to_ascii_lowercase() && !base_archive(name)
        })
}

fn base_archive(name: &str) -> bool {
    let numbered =
        |suffix: &str| suffix.len() == 2 && suffix.bytes().all(|byte| byte.is_ascii_digit());
    name == "iw_cod2x_01"
        || name
            .strip_prefix("iw_")
            .is_some_and(|suffix| numbered(suffix) && suffix <= "15")
        || (name.starts_with("localized_")
            && name
                .rsplit_once("_iw")
                .is_some_and(|(_, suffix)| numbered(suffix)))
}

pub fn managed_path(path: &str) -> bool {
    matches!(path, "mss32.dll" | "mss32_original.dll") || iwd_path(path)
}

pub fn validate_manifest(manifest: &Manifest) -> Result<()> {
    if manifest.id != manifest.id.to_ascii_lowercase()
        || manifest.version != manifest.version.to_ascii_lowercase()
        || !package_key(&manifest.id)
        || !package_key(&manifest.version)
        || manifest.name.trim().is_empty()
        || manifest.files.is_empty()
        || manifest.files.len() > 64
        || !official_version(&manifest.base_version)
    {
        return Err("Invalid package name, version, base version, or file count.".into());
    }
    let mut paths = HashSet::new();
    for item in &manifest.files {
        if !managed_path(&item.path)
            || !paths.insert(item.path.to_ascii_lowercase())
            || !valid_hash(&item.sha256)
        {
            return Err(format!("Invalid or duplicate package file: {}", item.path).into());
        }
    }
    let contains = |path: &str| paths.contains(path);
    let official = manifest.id == OFFICIAL_ID;
    let supported = match manifest.kind.as_str() {
        "official" => {
            official
                && manifest.files.len() == 2
                && manifest.version == manifest.base_version
                && contains("mss32.dll")
                && contains("mss32_original.dll")
        }
        "runtime" => !official && contains("mss32.dll") && !contains("mss32_original.dll"),
        "addon" => !official && manifest.files.iter().all(|item| iwd_path(&item.path)),
        _ => false,
    };
    if !supported {
        return Err("Package contents do not match their declared type.".into());
    }
    Ok(())
}

pub fn builtin() -> Manifest {
    Manifest {
        id: OFFICIAL_ID.into(),
        name: "Official CoD2x".into(),
        version: STABLE.into(),
        description: "Upstream Windows client patch".into(),
        authors: vec!["Yctn".into(), "eyza".into()],
        release_notes: "Release build from the upstream CoD2x project.".into(),
        kind: "official".into(),
        base_version: STABLE.into(),
        files: vec![
            FileSpec {
                path: "mss32.dll".into(),
                sha256: LOADER.into(),
            },
            FileSpec {
                path: "mss32_original.dll".into(),
                sha256: ORIGINAL.into(),
            },
        ],
    }
}

pub fn sha256(mut input: impl Read) -> Result<String> {
    let mut digest = Sha256::new();
    io::copy(&mut input, &mut digest)?;
    Ok(format!("{:x}", digest.finalize()))
}

pub fn hash_file(path: &Path) -> Result<String> {
    sha256(File::open(path)?)
}

pub fn package_dir(data: &Path, manifest: &Manifest) -> PathBuf {
    data.join("packages")
        .join(&manifest.id)
        .join(&manifest.version)
}

pub fn read_manifest(dir: &Path) -> Result<Manifest> {
    let manifest = serde_json::from_reader(File::open(dir.join("manifest.json"))?)?;
    validate_manifest(&manifest)?;
    Ok(manifest)
}

pub fn installed_packages(data: &Path) -> Result<Vec<Manifest>> {
    let root = data.join("packages");
    if !root.exists() {
        return Ok(Vec::new());
    }
    let mut manifests = Vec::new();
    for package in fs::read_dir(root)? {
        let package = package?;
        if !package.file_type()?.is_dir() || !package_key(&package.file_name().to_string_lossy()) {
            continue;
        }
        for version in fs::read_dir(package.path())? {
            let version = version?;
            if version.file_type()?.is_dir() && version.path().join("manifest.json").is_file() {
                let Ok(manifest) = read_manifest(&version.path()) else {
                    continue;
                };
                if package_dir(data, &manifest) == version.path() {
                    manifests.push(manifest);
                }
            }
        }
    }
    manifests.sort_by(|a, b| (&a.name, &a.version).cmp(&(&b.name, &b.version)));
    Ok(manifests)
}

pub fn previous_mod_files(data: &Path) -> Result<Vec<String>> {
    let root = data.join("packages");
    if !root.exists() {
        return Ok(Vec::new());
    }
    let mut files = Vec::new();
    for group in fs::read_dir(root)? {
        let group = group?;
        if !group.file_type()?.is_dir() {
            continue;
        }
        for entry in fs::read_dir(group.path())? {
            let entry = entry?;
            if entry.file_type()?.is_dir()
                && entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".previous-")
            {
                if let Ok(manifest) = read_manifest(&entry.path().join("previous")) {
                    files.extend(manifest.files.into_iter().map(|item| item.path));
                }
            }
        }
    }
    Ok(files)
}

pub fn requested_package(data: &Path, id: &str, version: &str) -> Result<Manifest> {
    if !package_key(id) || !package_key(version) {
        return Err("Invalid package identifier.".into());
    }
    let manifest = read_manifest(&data.join("packages").join(id).join(version))?;
    if manifest.id != id || manifest.version != version {
        return Err("The selected package does not match its cached manifest.".into());
    }
    Ok(manifest)
}

pub fn verify_cached(data: &Path, manifest: &Manifest) -> Result<()> {
    for item in &manifest.files {
        let path = package_dir(data, manifest).join("files").join(&item.path);
        if hash_file(&path)? != item.sha256 {
            return Err(format!("Cached package checksum mismatch: {}", item.path).into());
        }
    }
    Ok(())
}

pub fn download_official(data: &Path, version: &str) -> Result<Manifest> {
    if !official_version(version) {
        return Err("Choose a stable CoD2x version.".into());
    }
    if let Ok(manifest) = requested_package(data, OFFICIAL_ID, version) {
        if verify_cached(data, &manifest).is_ok() {
            return Ok(manifest);
        }
    }
    let client = github::client()?;
    let row: GitHubRelease =
        github::document(&client, &format!("{API}/tags/v{version}"), 1024 * 1024)?;
    let release = release_from_github(row)
        .filter(|row| row.version == version)
        .ok_or("This release has no verified Windows package.")?;
    let bytes = github::verified_asset(
        &client,
        &release.url,
        release.download_size,
        &release.sha256,
    )?;

    let mut zip = ZipArchive::new(Cursor::new(bytes))?;
    let mut manifest = Manifest {
        version: version.into(),
        base_version: version.into(),
        release_notes: release.notes,
        files: Vec::new(),
        ..builtin()
    };
    let mut members = Vec::new();
    for name in ["mss32.dll", "mss32_original.dll"] {
        let matched: Vec<_> = zip
            .file_names()
            .filter(|member| member.rsplit('/').next() == Some(name))
            .map(str::to_owned)
            .collect();
        if matched.len() != 1 {
            return Err(format!("The release must contain one {name}.").into());
        }
        let mut entry = zip.by_name(&matched[0])?;
        if entry.size() > MAX_PACKAGE {
            return Err("A release file exceeds the package size limit.".into());
        }
        manifest.files.push(FileSpec {
            path: name.into(),
            sha256: sha256(&mut entry)?,
        });
        members.push(matched[0].clone());
    }
    if version == STABLE && manifest.files != builtin().files {
        return Err("The pinned release files differ from the known CoD2x build.".into());
    }
    cache(&mut zip, data, manifest, members, false)
}

pub fn import_zip(data: &Path, path: &Path) -> Result<Manifest> {
    let file = File::open(path)?;
    if file.metadata()?.len() > MAX_PACKAGE {
        return Err("Package exceeds the 512 MB import limit.".into());
    }
    import_archive(data, file, None)
}

pub fn import_archive(
    data: &Path,
    input: impl Read + io::Seek,
    expected_manifest: Option<&Manifest>,
) -> Result<Manifest> {
    let mut zip = ZipArchive::new(input)?;
    let manifest: Manifest = {
        let entry = zip.by_name("manifest.json")?;
        if entry.size() > 64 * 1024 {
            return Err("Package manifest exceeds 64 KB.".into());
        }
        serde_json::from_reader(entry)?
    };
    validate_manifest(&manifest)?;
    if expected_manifest.is_some_and(|expected| expected != &manifest) {
        return Err("The package manifest does not match the mod catalog.".into());
    }
    if manifest.id == OFFICIAL_ID {
        return Err("Download official CoD2x releases from the Official view.".into());
    }
    let members: Vec<_> = manifest
        .files
        .iter()
        .map(|file| format!("files/{}", file.path))
        .collect();
    let expected: HashSet<&str> = members
        .iter()
        .map(String::as_str)
        .chain(["manifest.json"])
        .collect();
    let mut seen = HashSet::new();
    for index in 0..zip.len() {
        let entry = zip.by_index(index)?;
        if entry.is_dir() {
            continue;
        }
        if !expected.contains(entry.name())
            || !seen.insert(entry.name().to_owned())
            || entry.is_symlink()
        {
            return Err(
                format!("Unlisted, duplicate, or linked ZIP entry: {}", entry.name()).into(),
            );
        }
    }
    cache(
        &mut zip,
        data,
        manifest,
        members,
        expected_manifest.is_some(),
    )
}

fn cache(
    zip: &mut ZipArchive<impl Read + io::Seek>,
    data: &Path,
    manifest: Manifest,
    members: Vec<String>,
    catalog_verified: bool,
) -> Result<Manifest> {
    validate_manifest(&manifest)?;
    let destination = package_dir(data, &manifest);
    let group = destination
        .parent()
        .ok_or("Package has no cache directory.")?;
    fs::create_dir_all(group)?;
    let lock = File::options()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(group.join(".write-lock"))?;
    fs2::FileExt::lock_exclusive(&lock)?;
    let existing = destination.exists().then(|| read_manifest(&destination));
    let preserve_previous = match &existing {
        Some(Ok(cached)) if cached == &manifest => {
            if verify_cached(data, cached).is_ok() {
                return Ok(cached.clone());
            }
            false
        }
        Some(_) if catalog_verified => true,
        Some(Ok(_)) => return Err(
            "This package ID and version already contain different files. Increment the version."
                .into(),
        ),
        Some(Err(_)) => return Err("The existing package manifest is damaged.".into()),
        None => false,
    };
    let staging = tempfile::Builder::new()
        .prefix(".staging-")
        .tempdir_in(group)?;
    let mut total: u64 = 0;
    for (spec, member) in manifest.files.iter().zip(members) {
        let mut entry = zip.by_name(&member)?;
        total = total.saturating_add(entry.size());
        if entry.is_symlink() || total > MAX_PACKAGE {
            return Err(
                "The package contains linked entries or exceeds the extracted size limit.".into(),
            );
        }
        let file = staging.path().join("files").join(&spec.path);
        if let Some(parent) = file.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut output = File::create(&file)?;
        let expected_size = entry.size();
        if io::copy(&mut (&mut entry).take(expected_size + 1), &mut output)? != expected_size {
            return Err(format!("Unexpected size for {}", spec.path).into());
        }
        output.sync_all()?;
        if hash_file(&file)? != spec.sha256 {
            return Err(format!("Package checksum mismatch: {}", spec.path).into());
        }
    }
    fs::write(
        staging.path().join("manifest.json"),
        serde_json::to_vec_pretty(&manifest)?,
    )?;
    if existing.is_some() {
        // Keep the old cache until the replacement lands. A different local
        // package is retained so the author can repackage it under its own ID.
        let backup = tempfile::Builder::new()
            .prefix(".previous-")
            .tempdir_in(group)?;
        let previous = backup.path().join("previous");
        fs::rename(&destination, &previous)?;
        if let Err(error) = fs::rename(staging.path(), &destination) {
            if let Err(rollback) = fs::rename(&previous, &destination) {
                let location = backup.keep();
                return Err(format!(
                    "Cache update failed: {error}. Previous files are at {}: {rollback}",
                    location.display()
                )
                .into());
            }
            return Err(error.into());
        }
        if preserve_previous {
            let _ = backup.keep();
        }
    } else {
        fs::rename(staging.path(), &destination)?;
    }
    Ok(manifest)
}
