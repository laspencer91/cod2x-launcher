# Working on CoD2x Launcher

The launcher source is a Windows Tauri app. Read [README.md](README.md) for development and [docs/publishing.md](docs/publishing.md) for the mod package and GitHub release format.

## Package a mod

Use **tools/New-ModPackage.ps1 -ReleaseAssets** with an explicit build output folder and payload paths. The script emits **mod.zip** and **mod.json** and prints the descriptor as JSON. Ask which files belong to the mod if the build layout is ambiguous. The published tag is **v<version>**.

The launcher’s **Publish a mod** flow calls `scan_mod_build` for candidate files and `export_mod_build` for the selected files. The PowerShell helper is the command-line entry point for agents. A clean release contains only the mod payload.

For a new listing, use the mod catalog's GitHub issue form. Publish assets in the author's mod repository. Obtain the user's instruction before submitting an issue or publishing a release on their behalf.

## Verify changes

Run `npm run build` and `cargo test --release --manifest-path src-tauri/Cargo.toml --features tauri/custom-protocol`. The ignored integration tests fetch the official CoD2x release and the synthetic public publishing fixture. Use temporary game layouts for file-switch tests.
