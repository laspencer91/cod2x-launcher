use crate::packages::{self, Manifest, Result, OFFICIAL_ID};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    process::Command,
};

const MP_HASH: &str = "41124c4620c1e96b8b908611a4135e3ccef4f675f56338d7f85bea6684afdf0e";
const PLAYTEST_IWD: &str = "main/zpam999_claude2.iwd";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Candidate {
    pub path: PathBuf,
    pub source: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub kind: String,
    pub name: String,
    pub version: Option<String>,
    pub package_id: Option<String>,
    pub official_version: Option<String>,
    pub extra_managed_files: Vec<String>,
    pub path: PathBuf,
    pub executable_verified: bool,
    pub files: Vec<ObservedFile>,
    pub notes: Vec<String>,
    pub recovery_pending: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ObservedFile {
    pub path: String,
    pub sha256: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct Journal {
    root: PathBuf,
    backup: String,
    files: Vec<Change>,
}

#[derive(Debug, Serialize, Deserialize)]
struct Change {
    path: String,
    before: Option<String>,
    after: Option<String>,
}

pub fn saved_install(data: &Path) -> Result<Option<PathBuf>> {
    match fs::read(data.join("installation.json")) {
        Ok(bytes) => Ok(Some(serde_json::from_slice(&bytes)?)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

pub fn confirm(data: &Path, path: &Path) -> Result<Status> {
    let root = validated_root(path)?;
    let status = inspect(data, &root)?;
    fs::create_dir_all(data)?;
    write_atomic(&data.join("installation.json"), &serde_json::to_vec(&root)?)?;
    Ok(status)
}

fn regular_metadata(path: &Path, expect_directory: bool) -> Result<bool> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error.into()),
    };
    #[cfg(windows)]
    let reparse = {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes() & 0x400 != 0
    };
    #[cfg(not(windows))]
    let reparse = metadata.file_type().is_symlink();
    if reparse
        || (expect_directory && !metadata.is_dir())
        || (!expect_directory && !metadata.is_file())
    {
        return Err(format!(
            "Expected a regular {}: {}",
            if expect_directory {
                "directory"
            } else {
                "file"
            },
            path.display()
        )
        .into());
    }
    Ok(true)
}

fn validated_root(path: &Path) -> Result<PathBuf> {
    if !path.is_absolute() {
        return Err("Choose the full path to your Call of Duty 2 installation.".into());
    }
    let root = fs::canonicalize(path)?;
    if !regular_metadata(&root.join("main"), true)?
        || !regular_metadata(&root.join("CoD2MP_s.exe"), false)?
        || !regular_metadata(&root.join("main").join("iw_01.iwd"), false)?
        || !regular_metadata(&root.join("main").join("iw_15.iwd"), false)?
    {
        return Err("This folder is missing CoD2MP_s.exe or base game archives in main.".into());
    }
    let mut magic = [0; 2];
    File::open(root.join("CoD2MP_s.exe"))?.read_exact(&mut magic)?;
    if &magic != b"MZ" {
        return Err("CoD2MP_s.exe is not a Windows executable.".into());
    }
    #[cfg(windows)]
    {
        // Canonicalization prepends the Win32 extended-path prefix. Keep drive paths
        // readable in the UI while retaining UNC support.
        let value = root.to_string_lossy();
        if let Some(unc) = value.strip_prefix(r"\\?\UNC\") {
            return Ok(PathBuf::from(format!(r"\\{unc}")));
        }
        if let Some(drive) = value.strip_prefix(r"\\?\") {
            return Ok(PathBuf::from(drive));
        }
    }
    Ok(root)
}

pub fn discover() -> Vec<Candidate> {
    let mut candidates = Vec::new();
    #[cfg(windows)]
    {
        use winreg::{enums::HKEY_LOCAL_MACHINE, RegKey};
        let machine = RegKey::predef(HKEY_LOCAL_MACHINE);
        for key in [
            r"SOFTWARE\WOW6432Node\Activision\Call of Duty 2",
            r"SOFTWARE\Activision\Call of Duty 2",
        ] {
            if let Ok(registry) = machine.open_subkey(key) {
                for value in ["InstallPath", "InstallDir"] {
                    if let Ok(path) = registry.get_value::<String, _>(value) {
                        candidates.push((PathBuf::from(path), "Windows registry"));
                    }
                }
            }
        }
        if let Some(steam) = steam_root() {
            let mut libraries = vec![steam.clone()];
            if let Ok(text) = fs::read_to_string(steam.join("steamapps").join("libraryfolders.vdf"))
            {
                libraries.extend(
                    text.lines()
                        .filter_map(|line| vdf_value(line, "path"))
                        .map(PathBuf::from),
                );
            }
            for library in libraries {
                let apps = library.join("steamapps");
                let name = fs::read_to_string(apps.join("appmanifest_2630.acf"))
                    .ok()
                    .and_then(|text| text.lines().find_map(|line| vdf_value(line, "installdir")))
                    .filter(|name| {
                        !name.contains(['/', '\\', ':']) && !matches!(name.as_str(), "." | "..")
                    })
                    .unwrap_or_else(|| "Call of Duty 2".into());
                candidates.insert(0, (apps.join("common").join(name), "Steam library"));
            }
        }
        if let Some(program_files) = std::env::var_os("ProgramFiles(x86)") {
            candidates.push((
                PathBuf::from(program_files)
                    .join("Activision")
                    .join("Call of Duty 2"),
                "Retail folder",
            ));
        }
    }

    let mut found = BTreeSet::new();
    candidates
        .into_iter()
        .filter_map(|(path, source)| {
            let path = validated_root(&path).ok()?;
            found
                .insert(path.to_string_lossy().to_ascii_lowercase())
                .then(|| Candidate {
                    path,
                    source: source.to_owned(),
                })
        })
        .collect()
}

#[cfg(windows)]
fn steam_root() -> Option<PathBuf> {
    use winreg::{enums::HKEY_CURRENT_USER, RegKey};
    let user = RegKey::predef(HKEY_CURRENT_USER);
    user.open_subkey(r"Software\Valve\Steam")
        .ok()
        .and_then(|registry| registry.get_value::<String, _>("SteamPath").ok())
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("ProgramFiles(x86)").map(|base| PathBuf::from(base).join("Steam"))
        })
}

#[cfg(windows)]
fn vdf_value(line: &str, key: &str) -> Option<String> {
    let quoted: Vec<_> = line.split('"').collect();
    if quoted.len() == 5 && quoted[1].eq_ignore_ascii_case(key) {
        Some(quoted[3].replace(r"\\", r"\"))
    } else {
        None
    }
}

fn file_hash(root: &Path, path: &str) -> Result<Option<String>> {
    if !packages::managed_path(path) {
        return Err(format!("Unsupported game path: {path}").into());
    }
    let file = root.join(path);
    if path.starts_with("main/") && !regular_metadata(&root.join("main"), true)? {
        return Err("The main game folder is missing.".into());
    }
    if regular_metadata(&file, false)? {
        Ok(Some(packages::hash_file(&file)?))
    } else {
        Ok(None)
    }
}

fn installation_state(data: &Path, root: &Path) -> Result<PathBuf> {
    let hash = packages::sha256(root.to_string_lossy().to_lowercase().as_bytes())?;
    Ok(data.join("installations").join(hash))
}

fn tracked_paths(data: &Path, manifests: &[Manifest]) -> Result<BTreeSet<String>> {
    Ok(manifests
        .iter()
        .flat_map(|manifest| manifest.files.iter().map(|item| item.path.clone()))
        .chain(packages::previous_mod_files(data)?)
        .chain([
            "mss32.dll".into(),
            "mss32_original.dll".into(),
            PLAYTEST_IWD.into(),
        ])
        .collect())
}

fn all_manifests(data: &Path) -> Result<Vec<Manifest>> {
    let mut known = packages::installed_packages(data)?;
    if !known
        .iter()
        .any(|manifest| manifest.id == OFFICIAL_ID && manifest.version == packages::STABLE)
    {
        known.push(packages::builtin());
    }
    Ok(known)
}

fn expected<'a>(
    manifest: &'a Manifest,
    manifests: &'a [Manifest],
) -> Result<BTreeMap<String, &'a str>> {
    let mut files = BTreeMap::new();
    if manifest.id != OFFICIAL_ID {
        let base = manifests
            .iter()
            .find(|item| item.id == OFFICIAL_ID && item.version == manifest.base_version)
            .ok_or_else(|| {
                format!(
                    "Download official CoD2x {} before applying this build.",
                    manifest.base_version
                )
            })?;
        files.extend(
            base.files
                .iter()
                .map(|item| (item.path.clone(), item.sha256.as_str())),
        );
    }
    files.extend(
        manifest
            .files
            .iter()
            .map(|item| (item.path.clone(), item.sha256.as_str())),
    );
    Ok(files)
}

pub fn inspect(data: &Path, root: &Path) -> Result<Status> {
    let root = validated_root(root)?;
    let executable_verified = packages::hash_file(&root.join("CoD2MP_s.exe"))? == MP_HASH;
    let known = all_manifests(data)?;
    let files: BTreeMap<_, _> = tracked_paths(data, &known)?
        .into_iter()
        .map(|path| Ok((path.clone(), file_hash(&root, &path)?)))
        .collect::<Result<_>>()?;
    let official_versions: Vec<_> = known
        .iter()
        .filter(|manifest| {
            manifest.id == OFFICIAL_ID
                && manifest.files.iter().all(|file| {
                    files.get(&file.path).and_then(|sha| sha.as_deref())
                        == Some(file.sha256.as_str())
                })
        })
        .collect();
    let official_version = match official_versions.as_slice() {
        [manifest] => Some(manifest.version.clone()),
        _ => None,
    };
    let extra_managed_files: Vec<_> = files
        .iter()
        .filter(|(path, hash)| path.starts_with("main/") && hash.is_some())
        .map(|(path, _)| path.clone())
        .collect();
    let matches: Vec<_> = known
        .iter()
        .filter(|manifest| {
            expected(manifest, &known).is_ok_and(|expected| {
                files
                    .iter()
                    .all(|(path, sha)| sha.as_deref() == expected.get(path).copied())
            })
        })
        .collect();
    let mut notes = Vec::new();
    if !executable_verified {
        notes.push("The installation has the expected layout. This executable differs from the known Steam build.".into());
    }
    let recovery_pending = installation_state(data, &root)?
        .join("pending.json")
        .is_file();
    let (kind, name, version, package_id) = if recovery_pending {
        notes.push(
            "A file switch was interrupted. Restore the previous files before launching.".into(),
        );
        ("recovery", "Recovery needed".into(), None, None)
    } else if let [matched] = matches.as_slice() {
        (
            matched.kind.as_str(),
            matched.name.clone(),
            Some(matched.version.clone()),
            Some(matched.id.clone()),
        )
    } else {
        if let Some(version) = &official_version {
            if !extra_managed_files.is_empty() {
                notes.push(format!(
                    "The official CoD2x {version} DLLs match. Extra managed files: {}.",
                    extra_managed_files.join(", ")
                ));
            }
        } else if files.get(PLAYTEST_IWD).is_some_and(Option::is_some) {
            notes.push(format!(
                "The 125 Hz playtest archive is present at {PLAYTEST_IWD}."
            ));
        }
        if matches.len() > 1 {
            notes.push("More than one release has these exact files.".into());
        } else {
            notes.push("The tracked files differ from the known release combinations.".into());
        }
        ("mixed", "Mixed or unknown setup".into(), None, None)
    };
    let files = files
        .into_iter()
        .map(|(path, sha256)| ObservedFile { path, sha256 })
        .collect();
    Ok(Status {
        kind: kind.into(),
        name,
        version,
        package_id,
        official_version,
        extra_managed_files,
        path: root,
        executable_verified,
        files,
        notes,
        recovery_pending,
    })
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut temporary =
        tempfile::NamedTempFile::new_in(path.parent().ok_or("Missing parent directory.")?)?;
    temporary.write_all(bytes)?;
    temporary.as_file().sync_all()?;
    temporary.persist(path)?;
    Ok(())
}

fn copy_atomic(source: &Path, destination: &Path, sha: &str) -> Result<()> {
    let mut temporary =
        tempfile::NamedTempFile::new_in(destination.parent().ok_or("Missing parent directory.")?)?;
    io::copy(&mut File::open(source)?, &mut temporary)?;
    temporary.as_file().sync_all()?;
    if packages::hash_file(temporary.path())? != sha {
        return Err(format!("File changed while copying: {}", source.display()).into());
    }
    temporary.persist(destination)?;
    Ok(())
}

fn with_lock<T>(state: &Path, action: impl FnOnce() -> Result<T>) -> Result<T> {
    fs::create_dir_all(state)?;
    let lock = File::options()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(state.join("lock"))?;
    fs2::FileExt::try_lock_exclusive(&lock)
        .map_err(|_| "Another launcher operation is modifying this installation.")?;
    action()
}

pub fn restore_official(data: &Path, root: &Path, version: &str) -> Result<Status> {
    ensure_closed()?;
    let status = inspect(data, root)?;
    if status.recovery_pending {
        return Err("Recover the previous files before restoring official CoD2x.".into());
    }
    if status.official_version.as_deref() != Some(version) {
        packages::download_official(data, version)?;
    }
    apply(data, root, OFFICIAL_ID, version)
}

pub fn apply(data: &Path, root: &Path, id: &str, version: &str) -> Result<Status> {
    let root = validated_root(root)?;
    let state = installation_state(data, &root)?;
    with_lock(&state, || {
        ensure_closed()?;
        let pending = state.join("pending.json");
        if pending.exists() {
            return Err(
                "Restore the interrupted transaction before applying another setup.".into(),
            );
        }
        let target = if id == OFFICIAL_ID && version == packages::STABLE {
            packages::builtin()
        } else {
            packages::requested_package(data, id, version)?
        };
        if id != OFFICIAL_ID {
            packages::verify_cached(data, &target)?;
        }
        let manifests = all_manifests(data)?;
        let desired = expected(&target, &manifests)?;
        let sources: BTreeMap<_, _> = desired
            .keys()
            .map(|path| {
                let owner = if target.files.iter().any(|item| &item.path == path) {
                    &target
                } else {
                    manifests
                        .iter()
                        .find(|item| item.id == OFFICIAL_ID && item.version == target.base_version)
                        .expect("base manifest resolved while computing desired files")
                };
                (
                    path.clone(),
                    packages::package_dir(data, owner).join("files").join(path),
                )
            })
            .collect();
        let mut changes = Vec::new();
        for path in tracked_paths(data, &manifests)? {
            let before = file_hash(&root, &path)?;
            let after = desired.get(&path).map(|sha| (*sha).to_owned());
            if before != after {
                if let Some(sha) = &after {
                    let source = &sources[&path];
                    if !source.is_file() || packages::hash_file(source)? != *sha {
                        return Err(format!("Download official CoD2x {} or repair the cache before changing {path}.", target.base_version).into());
                    }
                }
                changes.push(Change {
                    path,
                    before,
                    after,
                });
            }
        }
        if changes.is_empty() {
            return inspect(data, &root);
        }

        let backups = state.join("backups");
        fs::create_dir_all(&backups)?;
        let backup = tempfile::Builder::new()
            .prefix("setup-")
            .tempdir_in(&backups)?;
        for change in &changes {
            if let Some(sha) = &change.before {
                let destination = backup.path().join(&change.path);
                fs::create_dir_all(destination.parent().ok_or("Invalid backup file path.")?)?;
                copy_atomic(&root.join(&change.path), &destination, sha)?;
            }
        }
        let backup = backup.keep();
        let journal = Journal {
            root: root.clone(),
            backup: backup
                .file_name()
                .ok_or("Missing backup name.")?
                .to_string_lossy()
                .into_owned(),
            files: changes,
        };
        write_atomic(&pending, &serde_json::to_vec_pretty(&journal)?)?;
        let applied = (|| -> Result<()> {
            for change in &journal.files {
                if file_hash(&root, &change.path)? != change.before {
                    return Err(format!("Game file changed during setup: {}", change.path).into());
                }
                let path = root.join(&change.path);
                if let Some(sha) = &change.after {
                    copy_atomic(&sources[&change.path], &path, sha)?;
                } else {
                    fs::remove_file(path)?;
                }
            }
            for change in &journal.files {
                if file_hash(&root, &change.path)? != change.after {
                    return Err(format!("Post-setup verification failed: {}", change.path).into());
                }
            }
            fs::remove_file(&pending)?;
            Ok(())
        })();
        if let Err(error) = applied {
            return match restore(&state, &root) {
                Ok(()) => {
                    Err(format!("Setup failed. Previous files were restored. {error}").into())
                }
                Err(recovery) => {
                    Err(format!("Setup failed: {error}. Recovery is pending: {recovery}").into())
                }
            };
        }
        inspect(data, &root)
    })
}

fn restore(state: &Path, root: &Path) -> Result<()> {
    let pending = state.join("pending.json");
    let journal: Journal = serde_json::from_slice(&fs::read(&pending)?)?;
    if journal.root != root
        || !journal.backup.starts_with("setup-")
        || !journal
            .backup
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-')
    {
        return Err("Recovery record does not belong to this installation.".into());
    }
    let backup = state.join("backups").join(&journal.backup);
    for change in &journal.files {
        let actual = file_hash(root, &change.path)?;
        if actual != change.before && actual != change.after {
            return Err(format!(
                "Preserved an external change to {}. Review it before recovery.",
                change.path
            )
            .into());
        }
        if let Some(sha) = &change.before {
            if packages::hash_file(&backup.join(&change.path))? != *sha {
                return Err(format!("Backup checksum mismatch: {}", change.path).into());
            }
        }
    }
    for change in &journal.files {
        if file_hash(root, &change.path)? == change.before {
            continue;
        }
        let destination = root.join(&change.path);
        if let Some(sha) = &change.before {
            copy_atomic(&backup.join(&change.path), &destination, sha)?;
        } else {
            fs::remove_file(destination)?;
        }
    }
    fs::remove_file(pending)?;
    Ok(())
}

pub fn recover(data: &Path, root: &Path) -> Result<Status> {
    let root = validated_root(root)?;
    let state = installation_state(data, &root)?;
    with_lock(&state, || {
        ensure_closed()?;
        restore(&state, &root)?;
        inspect(data, &root)
    })
}

pub fn launch(data: &Path, root: &Path, id: &str, version: &str) -> Result<()> {
    let root = validated_root(root)?;
    let state = installation_state(data, &root)?;
    with_lock(&state, || {
        ensure_closed()?;
        let status = inspect(data, &root)?;
        if status.package_id.as_deref() != Some(id) || status.version.as_deref() != Some(version) {
            return Err(
                "The active release changed. Verify the installation before launching.".into(),
            );
        }
        let mut game = Command::new(root.join("CoD2MP_s.exe"));
        game.current_dir(&root);
        #[cfg(windows)]
        if status.executable_verified {
            if let Some(steam) = steam_root().filter(|folder| folder.join("Steam.dll").is_file()) {
                let inherited = std::env::var_os("PATH").unwrap_or_default();
                let paths = std::iter::once(steam).chain(std::env::split_paths(&inherited));
                game.env("PATH", std::env::join_paths(paths)?);
            }
        }
        game.spawn()?;
        Ok(())
    })
}

#[cfg(windows)]
fn ensure_closed() -> Result<()> {
    use std::os::windows::process::CommandExt;
    let windows =
        std::env::var_os("SystemRoot").ok_or("Windows system directory is unavailable.")?;
    let output = Command::new(PathBuf::from(windows).join("System32").join("tasklist.exe"))
        .args(["/FO", "CSV", "/NH"])
        .creation_flags(0x08000000)
        .output()?;
    if !output.status.success() {
        return Err("Could not check whether Call of Duty 2 is running.".into());
    }
    let rows = String::from_utf8_lossy(&output.stdout);
    if rows
        .lines()
        .filter_map(|line| line.split('"').nth(1))
        .any(|name| {
            ["CoD2MP_s.exe", "CoD2SP_s.exe"]
                .iter()
                .any(|game| name.eq_ignore_ascii_case(game))
        })
    {
        return Err("Close Call of Duty 2 before switching or launching a setup.".into());
    }
    Ok(())
}

#[cfg(not(windows))]
fn ensure_closed() -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::packages::FileSpec;
    use std::{io::Cursor, path::Path};

    fn spec(path: &str, bytes: &[u8]) -> FileSpec {
        FileSpec {
            path: path.into(),
            sha256: packages::sha256(bytes).unwrap(),
        }
    }

    fn fixture() -> (tempfile::TempDir, PathBuf, PathBuf, Manifest, Manifest) {
        let work = tempfile::tempdir().unwrap();
        let game = work.path().join("game");
        let data = work.path().join("launcher");
        fs::create_dir_all(game.join("main")).unwrap();
        fs::write(game.join("CoD2MP_s.exe"), b"MZ fixture").unwrap();
        fs::write(game.join("main/iw_01.iwd"), b"assets").unwrap();
        fs::write(game.join("main/iw_15.iwd"), b"assets").unwrap();
        fs::write(game.join("main/custom_map.iwd"), b"user archive").unwrap();
        let mut official = packages::builtin();
        official.version = "9.9.9.9".into();
        official.base_version = official.version.clone();
        official.files = vec![
            spec("mss32.dll", b"official"),
            spec("mss32_original.dll", b"miles"),
        ];
        let playtest = Manifest {
            id: "cod2x-claude2".into(),
            name: "125 Hz Playtest".into(),
            version: "0.1.0".into(),
            description: "Synthetic setup fixture".into(),
            authors: vec!["One".into(), "Two".into()],
            release_notes: String::new(),
            kind: "runtime".into(),
            base_version: official.version.clone(),
            files: vec![
                spec("mss32.dll", b"playtest"),
                spec(PLAYTEST_IWD, b"archive"),
            ],
        };
        cache_fixture(&data, &official, &[b"official", b"miles"]);
        cache_fixture(&data, &playtest, &[b"playtest", b"archive"]);
        fs::write(game.join("mss32.dll"), b"official").unwrap();
        fs::write(game.join("mss32_original.dll"), b"miles").unwrap();
        (work, game, data, official, playtest)
    }

    fn cache_fixture(root: &Path, manifest: &Manifest, payloads: &[&[u8]]) {
        let directory = packages::package_dir(root, manifest);
        for (spec, bytes) in manifest.files.iter().zip(payloads) {
            let target = directory.join("files").join(&spec.path);
            fs::create_dir_all(target.parent().unwrap()).unwrap();
            fs::write(target, bytes).unwrap();
        }
        fs::write(
            directory.join("manifest.json"),
            serde_json::to_vec(manifest).unwrap(),
        )
        .unwrap();
    }

    #[test]
    fn switches_and_restores_a_verified_setup() {
        let (_work, game, data, official, playtest) = fixture();
        assert_eq!(confirm(&data, &game).unwrap().kind, "official");
        fs::write(game.join(PLAYTEST_IWD), b"leftover").unwrap();
        let mixed = inspect(&data, &game).unwrap();
        assert_eq!(mixed.kind, "mixed");
        assert_eq!(
            mixed.official_version.as_deref(),
            Some(official.version.as_str())
        );
        assert_eq!(mixed.extra_managed_files, [PLAYTEST_IWD]);
        let active = apply(&data, &game, &playtest.id, &playtest.version).unwrap();
        assert_eq!(active.version.as_deref(), Some(playtest.version.as_str()));
        assert_eq!(fs::read(game.join("mss32.dll")).unwrap(), b"playtest");
        let restored = apply(&data, &game, &official.id, &official.version).unwrap();
        assert_eq!(restored.kind, "official");
        assert_eq!(fs::read(game.join("mss32.dll")).unwrap(), b"official");
        assert_eq!(fs::read(game.join("mss32_original.dll")).unwrap(), b"miles");
        assert!(!game.join(PLAYTEST_IWD).exists());
        assert_eq!(
            fs::read(game.join("main/custom_map.iwd")).unwrap(),
            b"user archive"
        );
    }

    #[test]
    fn restores_matching_official_dlls_without_needing_a_download() {
        let (_work, game, data, official, _) = fixture();
        let cached_dll = packages::package_dir(&data, &official).join("files/mss32.dll");
        fs::remove_file(cached_dll).unwrap();
        fs::write(game.join(PLAYTEST_IWD), b"test archive").unwrap();

        let status = restore_official(&data, &game, &official.version).unwrap();
        assert_eq!(status.kind, "official");
        assert!(!game.join(PLAYTEST_IWD).exists());
        assert_eq!(
            fs::read(game.join("main/custom_map.iwd")).unwrap(),
            b"user archive"
        );
        let backups = installation_state(&data, &status.path)
            .unwrap()
            .join("backups");
        let backup = fs::read_dir(backups)
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        assert_eq!(
            fs::read(backup.join(PLAYTEST_IWD)).unwrap(),
            b"test archive"
        );
    }

    #[test]
    fn packages_cannot_manage_stock_archives() {
        let (_work, game, data, _, mut playtest) = fixture();
        for path in [
            "main/iw_00.iwd",
            "main/iw_15.iwd",
            "main/localized_english_iw03.iwd",
            "main/iw_cod2x_01.iwd",
        ] {
            playtest.files[1] = spec(path, b"replacement");
            assert!(packages::validate_manifest(&playtest).is_err());
        }
        cache_fixture(&data, &playtest, &[b"playtest", b"replacement"]);
        assert_eq!(inspect(&data, &game).unwrap().kind, "official");

        playtest.files[1] = spec("main/iw_prosmoke_stock.iwd", b"custom archive");
        packages::validate_manifest(&playtest).unwrap();
    }

    #[test]
    fn interrupted_switch_restores_only_expected_files() {
        let (_work, game, data, _, _) = fixture();
        let game = validated_root(&game).unwrap();
        let state = installation_state(&data, &game).unwrap();
        let backup = state.join("backups/setup-fixture");
        fs::create_dir_all(&backup).unwrap();
        fs::write(backup.join("mss32.dll"), b"official").unwrap();
        let journal = Journal {
            root: game.clone(),
            backup: "setup-fixture".into(),
            files: vec![Change {
                path: "mss32.dll".into(),
                before: Some(spec("mss32.dll", b"official").sha256),
                after: Some(spec("mss32.dll", b"playtest").sha256),
            }],
        };
        fs::write(
            state.join("pending.json"),
            serde_json::to_vec(&journal).unwrap(),
        )
        .unwrap();
        fs::write(game.join("mss32.dll"), b"external").unwrap();
        assert!(recover(&data, &game)
            .unwrap_err()
            .to_string()
            .contains("Preserved an external change"));
        assert_eq!(fs::read(game.join("mss32.dll")).unwrap(), b"external");
        fs::write(game.join("mss32.dll"), b"playtest").unwrap();
        assert_eq!(recover(&data, &game).unwrap().kind, "official");
        assert!(!state.join("pending.json").exists());
    }

    #[test]
    fn imports_and_verifies_a_multiauthor_package() {
        let (_work, _, _, _, mut manifest) = fixture();
        manifest.files = vec![spec("mss32.dll", b"playtest")];
        let build = |payload: &[u8], extra: bool| {
            let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
            let options = zip::write::SimpleFileOptions::default();
            zip.start_file("manifest.json", options).unwrap();
            zip.write_all(&serde_json::to_vec(&manifest).unwrap())
                .unwrap();
            zip.start_file("files/mss32.dll", options).unwrap();
            zip.write_all(payload).unwrap();
            if extra {
                zip.start_file("files/../unexpected.dll", options).unwrap();
                zip.write_all(b"unexpected").unwrap();
            }
            zip.finish().unwrap()
        };
        let target = tempfile::tempdir().unwrap();
        assert!(packages::import_archive(target.path(), build(b"bad", false), None).is_err());
        assert!(packages::import_archive(target.path(), build(b"playtest", true), None).is_err());
        let mut wrong = manifest.clone();
        wrong.description = "Wrong catalog manifest".into();
        assert!(
            packages::import_archive(target.path(), build(b"playtest", false), Some(&wrong))
                .is_err()
        );
        let cached =
            packages::import_archive(target.path(), build(b"playtest", false), Some(&manifest))
                .unwrap();
        assert_eq!(cached.authors, ["One", "Two"]);
        packages::verify_cached(target.path(), &manifest).unwrap();

        let payload = packages::package_dir(target.path(), &manifest).join("files/mss32.dll");
        fs::write(&payload, b"damaged cached copy").unwrap();
        assert!(packages::verify_cached(target.path(), &manifest).is_err());
        packages::import_archive(target.path(), build(b"playtest", false), Some(&manifest))
            .unwrap();
        assert_eq!(fs::read(&payload).unwrap(), b"playtest");

        cache_fixture(target.path(), &wrong, &[b"local variant"]);
        packages::import_archive(target.path(), build(b"playtest", false), Some(&manifest))
            .unwrap();
        let group = target.path().join("packages").join(&manifest.id);
        let previous = fs::read_dir(group)
            .unwrap()
            .flatten()
            .find(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".previous-")
            })
            .unwrap();
        assert_eq!(
            fs::read(previous.path().join("previous/files/mss32.dll")).unwrap(),
            b"local variant"
        );
        assert_eq!(fs::read(payload).unwrap(), b"playtest");
    }

    #[test]
    #[ignore = "downloads a public GitHub release"]
    fn downloads_upstream_windows_package() {
        let target = tempfile::tempdir().unwrap();
        let catalog = packages::releases().unwrap();
        assert!(catalog
            .iter()
            .any(|release| release.version == packages::STABLE));
        let manifest = packages::download_official(target.path(), packages::STABLE).unwrap();
        assert_eq!(manifest.files, packages::builtin().files);
        packages::verify_cached(target.path(), &manifest).unwrap();
    }
}
