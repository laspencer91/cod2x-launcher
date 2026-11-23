use crate::{
    github,
    packages::{self, Result},
};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{self, BufRead, Read},
    path::{Path, PathBuf},
    process::{Command, Stdio},
};
use tauri::ipc::Channel;

const CLI_VERSION: &str = "2.105.0";
const CLI_HASH: &str = "77437defc10c5cf35ca0d25d626d58a5210de8ad10bf270c02df4630754bcd69";

fn managed_directory(data: &Path) -> PathBuf {
    data.join("github-cli").join(CLI_VERSION)
}

fn executable(data: &Path) -> Result<Option<PathBuf>> {
    // Only run the system installation or our integrity-checked copy.
    for variable in ["ProgramW6432", "ProgramFiles"] {
        if let Some(root) = std::env::var_os(variable) {
            let path = PathBuf::from(root).join("GitHub CLI/gh.exe");
            if path.is_file() {
                return Ok(Some(path));
            }
        }
    }
    let path = managed_directory(data).join("gh.exe");
    if path.is_file() && packages::hash_file(&path)? == CLI_HASH {
        return Ok(Some(path));
    }
    Ok(None)
}

fn install_cli(data: &Path) -> Result<PathBuf> {
    let target = managed_directory(data);
    fs::create_dir_all(&target)?;
    let url = format!("https://github.com/cli/cli/releases/download/v{CLI_VERSION}/gh_{CLI_VERSION}_windows_amd64.zip");
    let bytes = github::verified_asset(
        &github::client()?,
        &url,
        15212614,
        "8813a72c899900afaa10e3fec8fbc64b908c89dbfe837e95ed2fa500fc140627",
    )?;
    let mut archive = zip::ZipArchive::new(io::Cursor::new(bytes))?;
    let mut license = String::new();
    archive.by_name("LICENSE")?.read_to_string(&mut license)?;
    fs::write(target.join("LICENSE"), license)?;
    let mut staged = tempfile::NamedTempFile::new_in(&target)?;
    io::copy(&mut archive.by_name("bin/gh.exe")?, &mut staged)?;
    if packages::hash_file(staged.path())? != CLI_HASH {
        return Err("GitHub CLI verification failed.".into());
    }
    let path = target.join("gh.exe");
    staged.persist(&path)?;
    Ok(path)
}

fn command(path: &Path) -> Command {
    let mut cmd = Command::new(path);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000);
    }
    cmd.env("GH_HOST", "github.com")
        .env("GH_NO_UPDATE_NOTIFIER", "1")
        .env("NO_COLOR", "1")
        .env_remove("GH_DEBUG")
        .stdin(Stdio::null());
    cmd
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Repository {
    name: String,
    has_mod: bool,
}

#[derive(Serialize, Deserialize)]
pub struct Account {
    login: String,
    repositories: Vec<Repository>,
}

// Fetch identity, release permissions, and recent mod descriptors in the same paginated request.
const QUERY: &str = r#"query($endCursor:String) {
    viewer { login repositories(first:40,after:$endCursor,privacy:PUBLIC,
        affiliations:[OWNER,COLLABORATOR,ORGANIZATION_MEMBER],orderBy:{field:NAME,direction:ASC}) {
        pageInfo { hasNextPage endCursor }
        nodes { nameWithOwner viewerPermission isArchived isDisabled
            releases(first:10,orderBy:{field:CREATED_AT,direction:DESC}) {
                nodes { isDraft releaseAssets(first:1,name:"mod.json") { totalCount } }
            }
        }
    } }
}"#;

const PROJECT: &str = r#"{login: .data.viewer.login, repositories: [
    .data.viewer.repositories.nodes[] | select(.isArchived == false and .isDisabled == false)
    | select(.viewerPermission == "WRITE" or .viewerPermission == "MAINTAIN" or .viewerPermission == "ADMIN")
    | {name: .nameWithOwner, hasMod: any(.releases.nodes[]; .isDraft == false and .releaseAssets.totalCount > 0)}
]}"#;

pub fn account(data: &Path) -> Result<Option<Account>> {
    let Some(path) = executable(data)? else {
        return Ok(None);
    };
    let output = command(&path)
        .args([
            "api",
            "--hostname",
            "github.com",
            "graphql",
            "--paginate",
            "-f",
            &format!("query={QUERY}"),
            "--jq",
            PROJECT,
        ])
        .output()?;
    if !output.status.success() {
        let message = String::from_utf8_lossy(&output.stderr);
        if message.contains("gh auth login")
            || message.contains("HTTP 401")
            || message.contains("Bad credentials")
        {
            return Ok(None);
        }
        return Err(
            "Couldn’t load your GitHub repositories. Check your connection and try again.".into(),
        );
    }
    let mut pages = serde_json::Deserializer::from_slice(&output.stdout).into_iter::<Account>();
    let mut account = pages
        .next()
        .ok_or("GitHub returned no account information.")??;
    for page in pages {
        account.repositories.extend(page?.repositories);
    }
    Ok(Some(account))
}

pub fn connect(data: &Path, progress: Channel<String>) -> Result<()> {
    let path = match executable(data)? {
        Some(path) => path,
        None => install_cli(data)?,
    };
    let mut child = command(&path)
        .args([
            "auth",
            "login",
            "--web",
            "--clipboard=false",
            "--hostname",
            "github.com",
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()?;
    let stderr = child
        .stderr
        .take()
        .ok_or("Couldn’t start GitHub sign-in.")?;
    for line in io::BufReader::new(stderr).lines() {
        let Ok(line) = line else {
            break;
        };
        if let Some(code) = line.split_ascii_whitespace().last() {
            if code.len() == 9
                && code.as_bytes()[4] == b'-'
                && code
                    .bytes()
                    .enumerate()
                    .all(|(i, b)| i == 4 || b.is_ascii_uppercase() || b.is_ascii_digit())
            {
                if progress.send(code.to_owned()).is_err() {
                    let _ = child.kill();
                    break;
                }
            }
        }
    }
    if !child.wait()?.success() {
        return Err(
            "GitHub sign-in ended. You can connect again or enter a repository link.".into(),
        );
    }
    Ok(())
}

pub fn request_listing(data: &Path, repository: &str, mod_id: &str) -> Result<u64> {
    use crate::catalog;
    if !catalog::repo_slug(repository)
        || !packages::package_key(mod_id)
        || mod_id == packages::OFFICIAL_ID
    {
        return Err("Choose a public mod repository and enter its stable mod ID.".into());
    }
    let registry = catalog::configured_repo()?.ok_or("No mod catalog is configured.")?;
    let cli = executable(data)?.ok_or("Connect GitHub or use the listing form.")?;
    let title = format!("List {mod_id}");
    let fields = format!(
        "### Mod ID\n\n{mod_id}\n\n### Public mod repository\n\nhttps://github.com/{repository}"
    );
    #[derive(Deserialize)]
    struct Listing {
        body: String,
        number: u64,
    }
    let existing = command(&cli)
        .args([
            "issue",
            "list",
            "--repo",
            registry,
            "--state",
            "open",
            "--author",
            "@me",
            "--search",
            &format!("{mod_id} in:title"),
            "--json",
            "number,body",
        ])
        .output()?;
    if !existing.status.success() {
        return Err(
            "Couldn’t check your open requests. Connect GitHub or use the listing form.".into(),
        );
    }
    for listing in serde_json::from_slice::<Vec<Listing>>(&existing.stdout)? {
        if listing
            .body
            .replace("\r\n", "\n")
            .lines()
            .take(fields.lines().count())
            .eq(fields.lines())
        {
            return Ok(listing.number);
        }
    }
    let created = command(&cli)
        .args([
            "api",
            &format!("repos/{registry}/issues"),
            "-f",
            &format!("title={title}"),
            "-f",
            &format!("body={fields}"),
            "--jq",
            ".number",
        ])
        .output()?;
    if !created.status.success() {
        return Err("GitHub could not submit this request. Try the listing form.".into());
    }
    let number: u64 = std::str::from_utf8(&created.stdout)?.trim().parse()?;
    Ok(number)
}
