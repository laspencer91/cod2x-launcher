#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod account;
mod catalog;
mod github;
mod install;
mod packages;
mod publisher;

use serde::Serialize;
use std::path::PathBuf;
use tauri::{AppHandle, Manager};
use tauri_plugin_opener::OpenerExt;

type Reply<T> = std::result::Result<T, String>;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Dashboard {
    installation: Option<install::Status>,
    candidates: Vec<install::Candidate>,
    packages: Vec<packages::Manifest>,
}

fn data(app: &AppHandle) -> Reply<PathBuf> {
    app.path()
        .app_local_data_dir()
        .map_err(|error| error.to_string())
}

async fn background<T: Send + 'static>(
    operation: impl FnOnce() -> packages::Result<T> + Send + 'static,
) -> Reply<T> {
    tauri::async_runtime::spawn_blocking(operation)
        .await
        .map_err(|error| error.to_string())?
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn dashboard(app: AppHandle) -> Reply<Dashboard> {
    let data = data(&app)?;
    background(move || {
        let installation = install::saved_install(&data)?
            .map(|path| install::inspect(&data, &path))
            .transpose()?;
        Ok(Dashboard {
            installation,
            candidates: install::discover(),
            packages: packages::installed_packages(&data)?,
        })
    })
    .await
}

#[tauri::command]
async fn discover_installs() -> Reply<Vec<install::Candidate>> {
    background(|| Ok(install::discover())).await
}

#[tauri::command]
async fn confirm_install(app: AppHandle, path: PathBuf) -> Reply<install::Status> {
    let data = data(&app)?;
    background(move || install::confirm(&data, &path)).await
}

#[tauri::command]
async fn official_releases() -> Reply<Vec<packages::Release>> {
    background(packages::releases).await
}

#[tauri::command]
async fn download_release(app: AppHandle, version: String) -> Reply<packages::Manifest> {
    let data = data(&app)?;
    background(move || packages::download_official(&data, &version)).await
}

#[tauri::command]
async fn hosted_releases() -> Reply<Vec<catalog::HostedRelease>> {
    background(catalog::releases).await
}

#[tauri::command]
async fn download_mod(app: AppHandle, id: String, version: String) -> Reply<packages::Manifest> {
    let data = data(&app)?;
    background(move || catalog::download(&data, &id, &version)).await
}

#[tauri::command]
async fn scan_mod_build(app: AppHandle, path: PathBuf) -> Reply<Vec<publisher::BuildFile>> {
    let data = data(&app)?;
    background(move || publisher::scan(&data, &path)).await
}

#[tauri::command]
async fn export_mod_build(app: AppHandle, draft: publisher::Draft) -> Reply<publisher::Export> {
    let data = data(&app)?;
    background(move || publisher::export(&data, draft)).await
}

#[tauri::command]
fn publishing_catalog() -> Reply<Option<&'static str>> {
    catalog::configured_repo().map_err(|error| error.to_string())
}

#[tauri::command]
async fn publisher_account(app: AppHandle) -> Reply<Option<account::Account>> {
    let data = data(&app)?;
    background(move || account::account(&data)).await
}

#[tauri::command]
async fn connect_publisher(app: AppHandle, progress: tauri::ipc::Channel<String>) -> Reply<()> {
    let data = data(&app)?;
    background(move || account::connect(&data, progress)).await
}

#[tauri::command]
async fn published_mod(repository: String) -> Reply<packages::Manifest> {
    background(move || catalog::published_mod(&repository)).await
}

#[tauri::command]
fn open_publishing_link(
    app: AppHandle,
    kind: String,
    repository: String,
    mod_id: String,
    name: String,
    version: String,
) -> Reply<()> {
    let url = publisher::github_url(&kind, &repository, &mod_id, &name, &version)
        .map_err(|error| error.to_string())?;
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn import_package(app: AppHandle, path: PathBuf) -> Reply<packages::Manifest> {
    let data = data(&app)?;
    background(move || packages::import_zip(&data, &path)).await
}

#[tauri::command]
async fn apply_package(app: AppHandle, id: String, version: String) -> Reply<install::Status> {
    let data = data(&app)?;
    background(move || {
        let root =
            install::saved_install(&data)?.ok_or("Confirm your Call of Duty 2 folder first.")?;
        install::apply(&data, &root, &id, &version)
    })
    .await
}

#[tauri::command]
async fn restore_official(app: AppHandle, version: String) -> Reply<install::Status> {
    let data = data(&app)?;
    background(move || {
        let root =
            install::saved_install(&data)?.ok_or("Confirm your Call of Duty 2 folder first.")?;
        install::restore_official(&data, &root, &version)
    })
    .await
}

#[tauri::command]
async fn recover_install(app: AppHandle) -> Reply<install::Status> {
    let data = data(&app)?;
    background(move || {
        let root =
            install::saved_install(&data)?.ok_or("Confirm your Call of Duty 2 folder first.")?;
        install::recover(&data, &root)
    })
    .await
}

#[tauri::command]
async fn launch_game(app: AppHandle, id: String, version: String) -> Reply<()> {
    let data = data(&app)?;
    background(move || {
        let root =
            install::saved_install(&data)?.ok_or("Confirm your Call of Duty 2 folder first.")?;
        install::launch(&data, &root, &id, &version)
    })
    .await
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            dashboard,
            discover_installs,
            confirm_install,
            official_releases,
            download_release,
            hosted_releases,
            download_mod,
            scan_mod_build,
            export_mod_build,
            publishing_catalog,
            publisher_account,
            connect_publisher,
            published_mod,
            open_publishing_link,
            import_package,
            apply_package,
            restore_official,
            recover_install,
            launch_game
        ])
        .run(tauri::generate_context!())
        .expect("failed to start CoD2x Launcher");
}
