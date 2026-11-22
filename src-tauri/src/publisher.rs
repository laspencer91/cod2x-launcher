use crate::{
    catalog::{self, ModDescriptor},
    packages::{self, FileSpec, Manifest, Result, MAX_PACKAGE},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
};
use zip::{write::SimpleFileOptions, CompressionMethod, ZipWriter};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildFile {
    #[serde(flatten)]
    pub spec: FileSpec,
    pub size: u64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Draft {
    source: PathBuf,
    destination: PathBuf,
    id: String,
    name: String,
    version: String,
    description: String,
    authors: Vec<String>,
    release_notes: String,
    base_version: String,
    files: Vec<FileSpec>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Export {
    directory: PathBuf,
    archive: PathBuf,
    descriptor: PathBuf,
    tag: String,
    sha256: String,
    size: u64,
}

fn regular(path: &Path, directory: bool) -> Result<bool> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error.into()),
    };
    #[cfg(windows)]
    let linked = {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes() & 0x400 != 0
    };
    #[cfg(not(windows))]
    let linked = metadata.file_type().is_symlink();
    Ok(!linked
        && if directory {
            metadata.is_dir()
        } else {
            metadata.is_file()
        })
}

fn source_file(root: &Path, path: &str) -> Result<Option<(PathBuf, u64)>> {
    if !(path == "mss32.dll" || packages::iwd_path(path)) {
        return Err(format!("Unsupported release file: {path}").into());
    }
    if path.starts_with("main/") && !regular(&root.join("main"), true)? {
        return Ok(None);
    }
    let file = root.join(path);
    if !regular(&file, false)? {
        return Ok(None);
    }
    let size = file.metadata()?.len();
    Ok((size != 0 && size <= MAX_PACKAGE).then_some((file, size)))
}

fn payload(root: &Path, path: &str) -> Result<Option<BuildFile>> {
    let Some((file, size)) = source_file(root, path)? else {
        return Ok(None);
    };
    Ok(Some(BuildFile {
        spec: FileSpec {
            path: path.into(),
            sha256: packages::hash_file(&file)?,
        },
        size,
    }))
}

fn official_dll(data: &Path, sha256: &str) -> Result<bool> {
    let mut packages = packages::installed_packages(data)?;
    packages.push(packages::builtin());
    Ok(packages
        .iter()
        .filter(|item| item.id == packages::OFFICIAL_ID)
        .any(|item| item.files.iter().any(|file| file.sha256 == sha256)))
}

pub fn scan(data: &Path, source: &Path) -> Result<Vec<BuildFile>> {
    if !source.is_absolute() || !regular(source, true)? {
        return Err("Choose a regular build folder.".into());
    }
    let mut files = Vec::new();
    if let Some(loader) = payload(source, "mss32.dll")? {
        if !official_dll(data, &loader.spec.sha256)? {
            files.push(loader);
        }
    }
    let main = source.join("main");
    if regular(&main, true)? {
        for item in fs::read_dir(&main)? {
            let item = item?;
            let name = item.file_name();
            if let Some(name) = name.to_str() {
                let path = format!("main/{name}");
                if packages::iwd_path(&path) {
                    if let Some(file) = payload(source, &path)? {
                        files.push(file);
                    }
                }
            }
        }
    }
    files.sort_by(|a, b| a.spec.path.cmp(&b.spec.path));
    Ok(files)
}

pub fn export(data: &Path, draft: Draft) -> Result<Export> {
    if !draft.destination.is_absolute() || !regular(&draft.destination, true)? {
        return Err("Choose a regular output folder.".into());
    }
    let kind = if draft.files.iter().any(|file| file.path == "mss32.dll") {
        "runtime"
    } else {
        "addon"
    };
    let manifest = Manifest {
        id: draft.id,
        name: draft.name,
        version: draft.version,
        description: draft.description,
        authors: draft.authors,
        release_notes: draft.release_notes,
        base_version: draft.base_version,
        files: draft.files,
        kind: kind.into(),
    };
    packages::validate_manifest(&manifest)?;
    if manifest.description.trim().is_empty()
        || !manifest.authors.iter().any(|name| !name.trim().is_empty())
    {
        return Err("Add a short description and at least one author credit.".into());
    }
    let metadata = serde_json::to_vec_pretty(&manifest)?;
    if metadata.len() > 60 * 1024 {
        return Err("Shorten the release metadata before exporting.".into());
    }
    if !draft.source.is_absolute() || !regular(&draft.source, true)? {
        return Err("Choose a regular build folder.".into());
    }
    let mut total = 0u64;
    let mut inputs = Vec::new();
    for file in &manifest.files {
        if file.path == "mss32.dll" && official_dll(data, &file.sha256)? {
            return Err("The selected DLL belongs to a known official CoD2x release.".into());
        }
        let (_, size) = source_file(&draft.source, &file.path)?
            .ok_or_else(|| format!("{} cannot be packaged. Scan the build again.", file.path))?;
        total = total.saturating_add(size);
        if total > MAX_PACKAGE {
            return Err("The selected files exceed the 512 MB package limit.".into());
        }
        inputs.push(size);
    }

    let output = draft
        .destination
        .join(format!("{}-v{}", manifest.id, manifest.version));
    if output.exists() {
        return Err("This version already exists in the output folder. Choose another folder or increment the version.".into());
    }
    let staging = tempfile::Builder::new()
        .prefix(".cod2x-export-")
        .tempdir_in(&draft.destination)?;
    let archive = staging.path().join("mod.zip");
    let mut zip = ZipWriter::new(File::create(&archive)?);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    zip.start_file("manifest.json", options)?;
    zip.write_all(&metadata)?;
    let mut buffer = [0u8; 64 * 1024];
    for (file, expected_size) in manifest.files.iter().zip(inputs) {
        let (path, size) = source_file(&draft.source, &file.path)?
            .filter(|(_, size)| *size == expected_size)
            .ok_or_else(|| format!("{} changed during export.", file.path))?;
        let mut input = File::open(path)?.take(size + 1);
        zip.start_file(format!("files/{}", file.path), options)?;
        let mut hash = Sha256::new();
        loop {
            let count = input.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            hash.update(&buffer[..count]);
            zip.write_all(&buffer[..count])?;
        }
        if format!("{:x}", hash.finalize()) != file.sha256 {
            return Err(format!("{} changed during export.", file.path).into());
        }
    }
    zip.finish()?.sync_all()?;
    let size = archive.metadata()?.len();
    if size > MAX_PACKAGE {
        return Err("The exported ZIP exceeds 512 MB.".into());
    }
    let sha256 = packages::hash_file(&archive)?;
    let tag = format!("v{}", manifest.version);
    let descriptor = ModDescriptor {
        manifest,
        asset: "mod.zip".into(),
        size,
        sha256: sha256.clone(),
    };
    fs::write(
        staging.path().join("mod.json"),
        serde_json::to_vec_pretty(&descriptor)?,
    )?;
    fs::rename(staging.path(), &output)?;
    Ok(Export {
        archive: output.join("mod.zip"),
        descriptor: output.join("mod.json"),
        directory: output,
        tag,
        sha256,
        size,
    })
}

pub fn github_url(
    kind: &str,
    repository: &str,
    mod_id: &str,
    name: &str,
    version: &str,
) -> Result<String> {
    if kind == "new" {
        let mut url = reqwest::Url::parse("https://github.com/new")?;
        if packages::package_key(mod_id) {
            url.query_pairs_mut().append_pair("name", mod_id);
        }
        return Ok(url.to_string());
    }
    if !catalog::repo_slug(repository) {
        return Err("Enter a public GitHub repository as owner/repository.".into());
    }
    let mut url = match kind {
        "release" => reqwest::Url::parse(&format!("https://github.com/{repository}/releases/new"))?,
        "listing" => {
            let registry = catalog::configured_repo()?
                .ok_or("The launcher has no configured mods catalog.")?;
            reqwest::Url::parse(&format!("https://github.com/{registry}/issues/new"))?
        }
        _ => return Err("Unknown publishing destination.".into()),
    };
    match kind {
        "release" => {
            url.query_pairs_mut()
                .append_pair("tag", &format!("v{version}"))
                .append_pair("title", &format!("{name} {version}"));
        }
        "listing" => {
            url.query_pairs_mut()
                .append_pair("template", "publisher-request.yml")
                .append_pair("title", &format!("List {name}"))
                .append_pair("mod", mod_id)
                .append_pair("repository", &format!("https://github.com/{repository}"));
        }
        _ => unreachable!(),
    }
    Ok(url.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn publisher_exports_only_the_selected_files_and_detects_changed_input() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("build");
        let destination = temp.path().join("release");
        fs::create_dir_all(source.join("main")).unwrap();
        fs::create_dir_all(&destination).unwrap();
        fs::write(source.join("main/iw_01.iwd"), b"stock fixture").unwrap();
        fs::write(source.join("main/map-rotation.iwd"), b"custom fixture").unwrap();
        fs::write(source.join("main/unused.iwd"), b"unselected fixture").unwrap();
        fs::write(source.join("main/config_mp.cfg"), b"config fixture").unwrap();
        let data = temp.path().join("launcher-data");

        let candidates = scan(&data, &source).unwrap();
        assert_eq!(candidates.len(), 2);
        assert_eq!(candidates[0].spec.path, "main/map-rotation.iwd");
        let make_draft = |version: &str| Draft {
            source: source.clone(),
            destination: destination.clone(),
            id: "map-rotation".into(),
            name: "Map Rotation".into(),
            version: version.into(),
            description: "Test fixture".into(),
            authors: vec!["Tester".into()],
            release_notes: String::new(),
            base_version: packages::STABLE.into(),
            files: vec![candidates[0].spec.clone()],
        };
        let result = export(&data, make_draft("0.1.0")).unwrap();
        let descriptor: ModDescriptor =
            serde_json::from_reader(File::open(result.descriptor).unwrap()).unwrap();
        assert_eq!(
            descriptor.sha256,
            packages::hash_file(&result.archive).unwrap()
        );
        let cached = packages::import_zip(&data, &result.archive).unwrap();
        assert_eq!(cached.files, vec![candidates[0].spec.clone()]);

        fs::write(source.join("main/map-rotation.iwd"), b"changed fixture").unwrap();
        assert!(export(&data, make_draft("0.2.0")).is_err());
        assert!(!destination.join("map-rotation-v0.2.0").exists());
    }
}
