use crate::packages::{self, Result};
use reqwest::blocking::Client;
use serde::{de::DeserializeOwned, Deserialize};
use std::{io::Read, time::Duration};

#[derive(Deserialize)]
pub struct Release {
    pub tag_name: String,
    pub name: Option<String>,
    pub body: Option<String>,
    pub published_at: Option<String>,
    pub draft: bool,
    pub prerelease: bool,
    pub assets: Vec<Asset>,
}

#[derive(Deserialize)]
pub struct Asset {
    pub name: String,
    pub size: u64,
    digest: Option<String>,
    pub browser_download_url: String,
}

impl Asset {
    pub fn sha256(&self) -> Option<String> {
        let digest = self
            .digest
            .as_deref()?
            .strip_prefix("sha256:")?
            .to_ascii_lowercase();
        packages::valid_hash(&digest).then_some(digest)
    }
}

pub fn client() -> Result<Client> {
    Ok(Client::builder()
        .user_agent("CoD2x-Launcher/0.1")
        .https_only(true)
        .connect_timeout(Duration::from_secs(8))
        .timeout(Duration::from_secs(45))
        .build()?)
}

pub fn document<T: DeserializeOwned>(client: &Client, url: &str, limit: u64) -> Result<T> {
    Ok(serde_json::from_slice(&bytes(client, url, limit)?)?)
}

pub fn verified_asset(client: &Client, url: &str, size: u64, sha256: &str) -> Result<Vec<u8>> {
    let contents = bytes(client, url, size)?;
    if contents.len() as u64 != size || packages::sha256(contents.as_slice())? != sha256 {
        return Err("The GitHub asset does not match its expected size or SHA-256.".into());
    }
    Ok(contents)
}

fn bytes(client: &Client, url: &str, limit: u64) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    client
        .get(url)
        .send()?
        .error_for_status()?
        .take(limit + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err("The GitHub response exceeds the size limit.".into());
    }
    Ok(bytes)
}
