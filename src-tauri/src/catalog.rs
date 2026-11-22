use crate::github::{self, Release as GitHubRelease};
use crate::packages::{self, Manifest, Result, MAX_PACKAGE, OFFICIAL_ID};
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use std::{collections::HashSet, path::Path};

const CATALOG: &str = "laspencer91/cod2x-mod-catalog";
const PUBLIC_ASSET: &str = "mod.json";

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HostedRelease {
    #[serde(default)]
    pub published_at: Option<String>,
    pub manifest: Manifest,
    pub url: String,
    pub sha256: String,
    pub size: u64,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct ModDescriptor {
    pub manifest: Manifest,
    pub asset: String,
    pub sha256: String,
    pub size: u64,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Registry {
    Repositories { mods: Vec<Approved> },
    SharedReleases(Vec<HostedRelease>),
}

#[derive(Deserialize)]
struct Approved {
    id: String,
    repository: String,
}

pub fn repo_slug(repo: &str) -> bool {
    let Some((owner, name)) = repo.split_once('/') else {
        return false;
    };
    let valid = |part: &str| {
        !part.is_empty()
            && part.len() <= 100
            && !matches!(part, "." | "..")
            && part
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"-_.".contains(&byte))
    };
    valid(owner) && valid(name)
}

pub fn configured_repo() -> Result<Option<&'static str>> {
    let repo = option_env!("COD2X_MODS_REPOSITORY").unwrap_or(CATALOG);
    if repo.is_empty() {
        return Ok(None);
    }
    if !repo_slug(repo) {
        return Err("Set COD2X_MODS_REPOSITORY to GitHub owner/repository.".into());
    }
    Ok(Some(repo))
}

pub fn releases() -> Result<Vec<HostedRelease>> {
    let Some(repo) = configured_repo()? else {
        return Ok(Vec::new());
    };
    let client = github::client()?;
    let catalog: Registry = github::document(
        &client,
        &format!("https://raw.githubusercontent.com/{repo}/main/catalog.json"),
        1024 * 1024,
    )?;
    match catalog {
        Registry::Repositories { mods } => {
            if mods.len() > 50 {
                return Err("The catalog supports up to 50 mod repositories.".into());
            }
            let mut known = HashSet::new();
            let mut releases = Vec::new();
            let mut last_error = None;
            let mut reached = false;
            for source in mods {
                if source.id == OFFICIAL_ID
                    || !packages::package_key(&source.id)
                    || !repo_slug(&source.repository)
                    || !known.insert(source.id.clone())
                {
                    return Err(format!("Invalid catalog repository for {}.", source.id).into());
                }
                match repo_releases(&client, &source.repository, &source.id) {
                    Ok(list) => {
                        reached = true;
                        releases.extend(list);
                    }
                    Err(error) => {
                        last_error = Some(error);
                    }
                }
            }
            if !reached {
                if let Some(error) = last_error {
                    return Err(error);
                }
            }
            Ok(releases)
        }
        Registry::SharedReleases(entries) => {
            let mut known = HashSet::new();
            for entry in &entries {
                let manifest = &entry.manifest;
                packages::validate_manifest(manifest)?;
                let prefix = format!(
                    "https://github.com/{repo}/releases/download/{}-v{}/",
                    manifest.id, manifest.version
                );
                let filename = entry.url.strip_prefix(&prefix).unwrap_or_default();
                if manifest.id == OFFICIAL_ID
                    || !known.insert((&manifest.id, &manifest.version))
                    || !packages::valid_hash(&entry.sha256)
                    || entry.size == 0
                    || entry.size > MAX_PACKAGE
                    || !filename.ends_with(".zip")
                    || !filename
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
                {
                    return Err(format!(
                        "Invalid catalog entry: {} {}.",
                        manifest.id, manifest.version
                    )
                    .into());
                }
            }
            Ok(entries)
        }
    }
}

fn repo_releases(client: &Client, slug: &str, id: &str) -> Result<Vec<HostedRelease>> {
    let rows: Vec<GitHubRelease> = github::document(
        client,
        &format!("https://api.github.com/repos/{slug}/releases?per_page=12"),
        2 * 1024 * 1024,
    )?;
    let mut releases = Vec::new();
    for row in rows.into_iter().filter(|row| !row.draft) {
        if let Some(release) = read_release(client, slug, Some(id), row)? {
            releases.push(release);
        }
    }
    Ok(releases)
}

pub fn published_mod(slug: &str) -> Result<Manifest> {
    if !repo_slug(slug) {
        return Err("Enter a GitHub repository as owner/repository.".into());
    }
    let client = github::client()?;
    for page in 1.. {
        let rows: Vec<GitHubRelease> = github::document(
            &client,
            &format!("https://api.github.com/repos/{slug}/releases?per_page=30&page={page}"),
            8 * 1024 * 1024,
        )
        .map_err(|_| {
            "Couldn’t read the public releases. Check the repository link or try again later."
        })?;
        let last = rows.len() < 30;
        for row in rows.into_iter().filter(|row| !row.draft) {
            if let Some(release) = read_release(&client, slug, None, row)? {
                return Ok(release.manifest);
            }
        }
        if last {
            break;
        }
    }
    Err("This repository has no published launcher package. Use List a new mod for its first release.".into())
}

fn read_release(
    client: &Client,
    slug: &str,
    id: Option<&str>,
    row: GitHubRelease,
) -> Result<Option<HostedRelease>> {
    let Some(version) = row
        .tag_name
        .strip_prefix('v')
        .filter(|value| packages::package_key(value))
    else {
        return Ok(None);
    };
    let prefix = format!(
        "https://github.com/{slug}/releases/download/{}/",
        row.tag_name
    );
    let Some(metadata) = row.assets.iter().find(|asset| asset.name == PUBLIC_ASSET) else {
        return Ok(None);
    };
    if metadata.size > 64 * 1024
        || metadata.browser_download_url != format!("{prefix}{PUBLIC_ASSET}")
    {
        return Ok(None);
    }
    let Some(sha256) = metadata.sha256() else {
        return Ok(None);
    };
    let bytes = github::verified_asset(
        client,
        &metadata.browser_download_url,
        metadata.size,
        &sha256,
    )?;
    let descriptor: ModDescriptor = serde_json::from_slice(&bytes)?;
    packages::validate_manifest(&descriptor.manifest)?;
    let Some(archive) = row.assets.iter().find(|asset| asset.name == "mod.zip") else {
        return Ok(None);
    };
    if descriptor.manifest.id == OFFICIAL_ID
        || id.is_some_and(|expected| descriptor.manifest.id != expected)
        || descriptor.manifest.version != version
        || descriptor.asset != archive.name
        || archive.browser_download_url != format!("{prefix}mod.zip")
        || archive.size != descriptor.size
        || descriptor.size == 0
        || descriptor.size > MAX_PACKAGE
        || !packages::valid_hash(&descriptor.sha256)
        || archive.sha256().as_deref() != Some(descriptor.sha256.as_str())
    {
        return Ok(None);
    }
    Ok(Some(HostedRelease {
        published_at: row.published_at,
        manifest: descriptor.manifest,
        url: archive.browser_download_url.clone(),
        sha256: descriptor.sha256,
        size: descriptor.size,
    }))
}

fn prepare(data: &Path, entry: &HostedRelease) -> Result<Manifest> {
    if let Ok(cached) =
        packages::requested_package(data, &entry.manifest.id, &entry.manifest.version)
    {
        if cached == entry.manifest && packages::verify_cached(data, &cached).is_ok() {
            return Ok(cached);
        }
    }
    let bytes = github::verified_asset(&github::client()?, &entry.url, entry.size, &entry.sha256)?;
    packages::import_archive(data, std::io::Cursor::new(bytes), Some(&entry.manifest))
}

pub fn download(data: &Path, id: &str, version: &str) -> Result<Manifest> {
    let entry = releases()?
        .into_iter()
        .find(|release| release.manifest.id == id && release.manifest.version == version)
        .ok_or("This release is not in the configured mods catalog.")?;
    prepare(data, &entry)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "downloads a public GitHub test fixture"]
    fn reads_and_downloads_the_publisher_fixture() {
        let data = tempfile::tempdir().unwrap();
        let versions = repo_releases(
            &github::client().unwrap(),
            "laspencer91/cod2x-125hz-publisher-test",
            "publisher-fixture",
        )
        .unwrap();
        let release = versions
            .iter()
            .find(|release| release.manifest.version == "0.1.0")
            .unwrap();
        let manifest = prepare(data.path(), release).unwrap();
        assert_eq!(manifest.id, "publisher-fixture");
        packages::verify_cached(data.path(), &manifest).unwrap();
    }
}
