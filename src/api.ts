import { invoke, isTauri } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";

export const native = isTauri();
export const OFFICIAL = "official-cod2x";
export const STABLE = "1.4.6.8";
export const PLAYTEST = "cod2x-claude2";

export interface Candidate {
  path: string;
  source: string;
}

export interface Status {
  kind: "official" | "runtime" | "addon" | "mixed" | "recovery";
  name: string;
  version: string | null;
  packageId: string | null;
  officialVersion: string | null;
  extraManagedFiles: string[];
  path: string;
  executableVerified: boolean;
  files: { path: string; sha256: string | null }[];
  notes: string[];
  recoveryPending: boolean;
}

export interface FileSpec { path: string; sha256: string }

export interface Package {
  id: string;
  name: string;
  version: string;
  description: string;
  authors: string[];
  releaseNotes: string;
  kind: "official" | "runtime" | "addon";
  baseVersion: string;
  files: FileSpec[];
}

export interface Release {
  version: string;
  title: string;
  notes: string;
  publishedAt: string;
  downloadSize: number;
  sha256: string;
  url: string;
}

export interface HostedRelease {
  manifest: Package;
  size: number;
  sha256: string;
  url: string;
}

export interface Dashboard {
  installation: Status | null;
  candidates: Candidate[];
  packages: Package[];
}

const previewPath = "C:\\Games\\Call of Duty 2";

export const previewRelease: Release = {
  version: STABLE,
  title: "CoD2x " + STABLE,
  notes: "New machine-gun sensitivity control and a fix for the server's CoD2x identifier.",
  publishedAt: "2026-04-07",
  downloadSize: 2577745,
  sha256: "c33f52037e67eaa948746a93967d27d8c70ab58623abab48d07803600a6a8555",
  url: "https://github.com/callofduty2x/CoD2x/releases/tag/v1.4.6.8",
};

const previewPackage: Package = {
  id: OFFICIAL, name: "Official CoD2x", version: STABLE, kind: "official",
  description: "The upstream Windows client patch.",
  authors: ["Yctn", "eyza"], baseVersion: STABLE, releaseNotes: previewRelease.notes,
  files: [
    { path: "mss32.dll", sha256: "b994ae0ee542490bb16f28a217c4914da7fdea413232d4e43d1e28ff46917cac" },
    { path: "mss32_original.dll", sha256: "7855b8fbae917cb8449f2d4361ab61b5ecec4df0a11130d797cb0aa99b4260ea" },
  ],
};

const previewCandidate = { path: previewPath, source: "Steam library" };

export const previewDashboard: Dashboard = {
  candidates: [previewCandidate],
  installation: {
    path: previewPath, kind: "official", name: "Official CoD2x", version: STABLE,
    packageId: OFFICIAL, officialVersion: STABLE, extraManagedFiles: [], executableVerified: true, recoveryPending: false,
    notes: [], files: [...previewPackage.files, { path: "main/zpam999_claude2.iwd", sha256: null }],
  },
  packages: [],
};

const previewParams = new URLSearchParams(window.location.search);
let previewInstallation = previewParams.has("first-run") ? null
  : previewParams.has("playtest") ? {
    ...previewDashboard.installation!,
    kind: "mixed" as const,
    name: "Mixed or unknown setup",
    packageId: null,
    version: null,
    extraManagedFiles: ["main/zpam999_claude2.iwd"],
    notes: ["The official CoD2x DLLs match and the 125 Hz playtest archive is still installed."],
  } : previewDashboard.installation;

function getPreviewDashboard(): Dashboard {
  return { ...previewDashboard, installation: previewInstallation };
}

export function initialDashboard(): Dashboard {
  return native ? { installation: null, candidates: [], packages: [] } : getPreviewDashboard();
}

export const api = {
  dashboard: () => native ? invoke<Dashboard>("dashboard") : Promise.resolve(getPreviewDashboard()),
  discover: () => native ? invoke<Candidate[]>("discover_installs") : Promise.resolve([previewCandidate]),
  releases: () => native ? invoke<Release[]>("official_releases") : Promise.resolve([previewRelease]),
  hosted: () => native ? invoke<HostedRelease[]>("hosted_releases") : Promise.resolve([]),
  downloadMod: (id: string, version: string) => invoke<Package>("download_mod", { id, version }),
  confirm: (path: string) => native
    ? invoke<Status>("confirm_install", { path })
    : Promise.resolve(previewInstallation = { ...previewDashboard.installation!, path }),
  download: (version: string) => native
    ? invoke<Package>("download_release", { version })
    : Promise.resolve(previewPackage),
  importZip: (path: string) => invoke<Package>("import_package", { path }),
  apply: (id: string, version: string) => native
    ? invoke<Status>("apply_package", { id, version })
    : Promise.resolve(previewDashboard.installation!),
  restoreOfficial: (version: string) => native
    ? invoke<Status>("restore_official", { version })
    : Promise.resolve(previewInstallation = { ...previewDashboard.installation!, version, officialVersion: version }),
  recover: () => invoke<Status>("recover_install"),
  launch: (id: string, version: string) => invoke<void>("launch_game", { id, version }),
  chooseFolder: (title = "Select your Call of Duty 2 folder") => native
    ? open({ directory: true, multiple: false, title })
    : Promise.resolve(previewPath),
  choosePackage: () => open({
    directory: false, multiple: false, title: "Import a CoD2x package",
    filters: [{ name: "CoD2x package ZIP", extensions: ["zip"] }],
  }),
};
