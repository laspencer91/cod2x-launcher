# CoD2x Launcher

A community Windows launcher for switching between official CoD2x and test builds. Pick a setup, play, and restore official CoD2x in one click when you’re done testing.

![CoD2x Launcher showing the Play button and setup chooser](docs/preview.png)

## Get started

1. [Download the Windows installer](https://github.com/laspencer91/cod2x-launcher/releases).
2. Open the launcher and confirm your Call of Duty 2 folder.
3. Play an official release, browse **Other Mods**, or import a package in **Local Builds**.

The launcher verifies official downloads and backs up the mod files it manages when switching setups. Bring your own Call of Duty 2 installation.

## Publish a mod

Open **Other Mods → Publish a mod**. Select your build files, export a ZIP and descriptor, and upload both to a release in your own GitHub repo. The editor can copy a packaging prompt for an AI agent. [Publishing guide](docs/publishing.md)

## Build from source

On Windows, install Node.js and the [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/). Run `npm ci`, then `npm run tauri dev`. Use `npm run tauri build` for a Windows installer.

[MIT licensed](LICENSE) · [Official CoD2x project](https://github.com/callofduty2x/CoD2x) · [Mod catalog](https://github.com/laspencer91/cod2x-mod-catalog)
