import { useEffect, useRef, useState } from "react";
import type { ReactNode } from "react";
import { PublishView } from "./PublishView";
import {
  ArrowDownToLine, ArrowRight, Box, Check, ChevronDown,
  ChevronRight, CircleAlert, CircleCheck, ClipboardCheck, Folder, FolderOpen,
  Gamepad2, Github, HardDrive, Layers3, LayoutGrid, Minus, PackagePlus,
  Play, RefreshCw, RotateCcw, ShieldCheck, Square, X,
} from "lucide-react";
import type { LucideIcon } from "lucide-react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  api, initialDashboard, native, OFFICIAL, PLAYTEST, previewRelease, STABLE,
} from "./api";
import type { Candidate, Dashboard, HostedRelease, Package, Release, Status } from "./api";

type Page = "overview" | "official" | "mods" | "local" | "installation" | "publish";
type Prompt = { type: "select"; candidate: Candidate } | { type: "apply"; release: Package };

const navigation: { id: Page; label: string; icon: LucideIcon }[] = [
  { id: "overview", label: "Overview", icon: LayoutGrid },
  { id: "official", label: "Official CoD2x", icon: ShieldCheck },
  { id: "mods", label: "Other Mods", icon: Layers3 },
  { id: "local", label: "Local Builds", icon: Box },
];

function StatusPill({ status }: { status: Status | null }) {
  const review = status?.kind === "mixed" || status?.kind === "recovery";
  const official = status?.kind === "official";
  return <span className={"status-pill " + (review ? "status-review" : official ? "status-good" : "status-neutral")}>
    <span className="status-dot" />
    {status?.recoveryPending ? "Recovery required" : review ? "Discrepancy found" : official ? "Official verified" : status ? "Mod active" : "Setup required"}
  </span>;
}

function Modal({ title, children, onClose }: { title: string; children: ReactNode; onClose: () => void }) {
  const dialog = useRef<HTMLDialogElement>(null);
  useEffect(() => {
    const element = dialog.current;
    element?.showModal();
    return () => element?.close();
  }, []);
  return <dialog ref={dialog} aria-labelledby="confirm-title" onCancel={(event) => { event.preventDefault(); onClose(); }}>
    <div className="dialog-heading">
      <h2 id="confirm-title">{title}</h2>
      <button className="icon-button" onClick={onClose} aria-label="Close dialog"><X size={17} /></button>
    </div>
    {children}
  </dialog>;
}

function PageTitle({ title, description, action }: { title: string; description?: string; action?: ReactNode }) {
  return <header className="page-heading">
    <div><h1>{title}</h1>{description && <p>{description}</p>}</div>
    {action}
  </header>;
}

function EmptyState({ icon: Icon, title, children }: { icon: LucideIcon; title: string; children: ReactNode }) {
  return <div className="empty-state"><div className="tile-icon"><Icon size={22} /></div><h2>{title}</h2>{children}</div>;
}

function AppWindowControls() {
  if (!native) return null;
  const window = getCurrentWindow();
  return <div className="window-buttons">
    <button onClick={() => void window.minimize()} className="window-button" aria-label="Minimize"><Minus size={15} /></button>
    <button onClick={() => void window.toggleMaximize()} className="window-button" aria-label="Maximize or restore"><Square size={12} /></button>
    <button onClick={() => void window.close()} className="window-button window-close" aria-label="Close"><X size={16} /></button>
  </div>;
}

const releaseDate = (value: string) => value
  ? new Intl.DateTimeFormat("en", { month: "short", day: "numeric", year: "numeric", timeZone: "UTC" }).format(new Date(value))
  : "Date unavailable";

function sameManifest(a: Package, b: Package) {
  return a.id === b.id && a.version === b.version && a.kind === b.kind && a.baseVersion === b.baseVersion
    && a.name === b.name && a.description === b.description && a.releaseNotes === b.releaseNotes
    && a.authors.length === b.authors.length && a.authors.every((author, index) => author === b.authors[index])
    && a.files.length === b.files.length
    && a.files.every((file, index) => file.path === b.files[index].path && file.sha256 === b.files[index].sha256);
}

function releaseSummary(text: string) {
  return text.split(/\r?\n/).map((line) => line.trim()).filter(Boolean).slice(0, 8);
}

export function App() {
  const [page, setPage] = useState<Page>("overview");
  const [data, setData] = useState<Dashboard>(initialDashboard);
  const [loadingInstallation, setLoadingInstallation] = useState(native);
  const [onboardingFolder, setOnboardingFolder] = useState<Candidate | null>(null);
  const [setupChoice, setSetupChoice] = useState("");
  const [releases, setReleases] = useState<Release[]>([previewRelease]);
  const [hosted, setHosted] = useState<HostedRelease[]>([]);
  const [catalogError, setCatalogError] = useState("");
  const [modVersions, setModVersions] = useState<Record<string, string>>({});
  const [versions, setVersions] = useState({ official: STABLE, local: "" });
  const [releaseMessage, setReleaseMessage] = useState(native ? "Checking GitHub" : "GitHub Releases");
  const [busy, setBusy] = useState("");
  const [feedback, setFeedback] = useState<{ kind: "error" | "info"; text: string } | null>(null);
  const [prompt, setPrompt] = useState<Prompt | null>(null);

  const installation = data.installation;
  const verified = installation?.packageId && installation.version && !installation.recoveryPending;
  const selectedRelease = releases.find((release) => release.version === versions.official) ?? releases[0];
  const selectedOfficial = data.packages.find((item) => item.id === OFFICIAL && item.version === selectedRelease.version);
  const cachedMods = data.packages.filter((item) => item.id !== OFFICIAL);
  const local = cachedMods.filter((item) => !hosted.some(({ manifest }) => sameManifest(item, manifest)));
  const selectedLocal = local.find((item) => item.id + "/" + item.version === versions.local) ?? local[0];
  const installationReady = installation !== null;
  const latest = releases[0];
  const latestVerified = releaseMessage === "GitHub Releases";
  const restoreVersion = latestVerified ? latest.version : installation?.officialVersion ?? latest.version;
  const setupKey = (id: string, version: string) => id + "/" + version;
  const officialVersions = [...new Set([
    ...releases.map((release) => release.version),
    ...data.packages.filter((item) => item.id === OFFICIAL).map((item) => item.version),
    ...(installation?.kind === "official" && installation.version ? [installation.version] : []),
  ])];
  const customPackages = new Map<string, Package>();
  for (const release of hosted) {
    const item = release.manifest;
    customPackages.set(setupKey(item.id, item.version), item);
  }
  for (const item of local) {
    const key = setupKey(item.id, item.version);
    if (!customPackages.has(key)) customPackages.set(key, item);
  }
  const activeKey = installation?.packageId && installation.version
    ? setupKey(installation.packageId, installation.version) : "";
  const chosenKey = setupChoice || activeKey || setupKey(OFFICIAL, restoreVersion);

  useEffect(() => {
    let current = true;
    api.dashboard().then((dashboard) => {
      if (current) setData(dashboard);
    }).catch(async (error: unknown) => {
      if (!current) return;
      setFeedback({ kind: "error", text: String(error) });
      const candidates = await api.discover().catch(() => []);
      if (current) setData((previous) => ({ ...previous, candidates }));
    }).finally(() => {
      if (current) setLoadingInstallation(false);
    });
    return () => { current = false; };
  }, []);

  useEffect(() => {
    if (!installationReady) return;
    let current = true;
    api.releases().then((available) => {
      if (!current) return;
      if (available.length) {
        setReleases(available);
        setVersions((previous) => ({ ...previous, official: available[0].version }));
      }
      setReleaseMessage(available.length ? "GitHub Releases" : "Pinned release");
    }).catch(() => { if (current) setReleaseMessage("Pinned release"); });
    api.hosted().then((available) => {
      if (current) setHosted(available);
    }).catch((error: unknown) => {
      if (current) setCatalogError(String(error));
    });
    return () => { current = false; };
  }, [installationReady]);

  function go(next: Page) {
    setPage(next);
    setFeedback(null);
  }

  function updateStatus(status: Status) {
    setData((previous) => ({ ...previous, installation: status }));
  }

  function addPackage(release: Package) {
    setData((previous) => ({
      ...previous,
      packages: [...previous.packages.filter((item) => item.id !== release.id || item.version !== release.version), release],
    }));
  }

  async function run(label: string, action: () => Promise<void>) {
    setBusy(label);
    setFeedback(null);
    try {
      await action();
    } catch (error) {
      setFeedback({ kind: "error", text: String(error) });
    } finally {
      setBusy("");
    }
  }

  function choose(candidate?: Candidate) {
    if (candidate) {
      setPrompt({ type: "select", candidate });
      return;
    }
    void run("Choosing folder", async () => {
      const path = await api.chooseFolder();
      if (typeof path !== "string") return;
      const selected = { path, source: "Selected folder" };
      if (installation) setPrompt({ type: "select", candidate: selected });
      else setOnboardingFolder(selected);
    });
  }

  function useInstallation(candidate: Candidate) {
    void run("Verifying installation", async () => {
      updateStatus(await api.confirm(candidate.path));
      setSetupChoice("");
      go("overview");
    });
  }

  async function refreshAfterFileError() {
    const updated = await api.dashboard().catch(() => null);
    if (updated) setData(updated);
  }

  function verify() {
    void run("Verifying files", async () => {
      const updated = await api.dashboard();
      setData(updated);
      if (updated.installation) {
        const checked = updated.installation;
        const detail = checked.recoveryPending ? "Recover the previous setup before launching."
          : checked.kind === "mixed" ? "This setup needs a restore before launching official CoD2x."
          : checked.name + (checked.version ? " " + checked.version : "") + " matches the managed files.";
        setFeedback({ kind: "info", text: detail });
      }
    });
  }

  function launch() {
    if (!installation?.packageId || !installation.version) return;
    if (!native) {
      setFeedback({ kind: "info", text: "The desktop build launches the detected CoD2x profile." });
      return;
    }
    const { packageId, version } = installation;
    void run("Launching CoD2", async () => {
      try {
        await api.launch(packageId, version);
      } catch (error) {
        await refreshAfterFileError();
        throw error;
      }
      setFeedback({ kind: "info", text: "Call of Duty 2 was started." });
    });
  }

  function prepareMod(release: Package) {
    void run("Checking " + release.name, async () => {
      const ready = await api.downloadMod(release.id, release.version);
      addPackage(ready);
      if (installation?.officialVersion !== ready.baseVersion) await download(ready.baseVersion);
      startSetup(ready);
    });
  }

  async function download(version: string) {
    const cached = await api.download(version);
    addPackage(cached);
    return cached;
  }

  function startSetup(release: Package) {
    if (!installation) {
      go("installation");
      return;
    }
    setPrompt({ type: "apply", release });
  }

  function setupOfficial(version: string) {
    if (!installation) {
      go("installation");
      return;
    }
    void run("Verifying official download", async () => {
      startSetup(await download(version));
    });
  }

  function restoreOfficial() {
    if (!installation || installation.recoveryPending) return;
    void run("Restoring official CoD2x", async () => {
      let status: Status;
      try {
        status = await api.restoreOfficial(restoreVersion);
      } catch (error) {
        await refreshAfterFileError();
        throw error;
      }
      updateStatus(status);
      if (status.kind !== "official" || status.version !== restoreVersion) {
        throw new Error("The official setup could not be verified. See File details under Installation.");
      }
      setSetupChoice(setupKey(OFFICIAL, restoreVersion));
      setFeedback({ kind: "info", text: "Official CoD2x " + restoreVersion + " restored and verified." });
    });
  }

  function importZip() {
    if (!native) {
      setFeedback({ kind: "info", text: "Open the Windows desktop build to import a package from your PC." });
      return;
    }
    void run("Importing package", async () => {
      const path = await api.choosePackage();
      if (typeof path !== "string") return;
      const release = await api.importZip(path);
      addPackage(release);
      setVersions((previous) => ({ ...previous, local: release.id + "/" + release.version }));
      setPage("local");
      setFeedback({ kind: "info", text: release.name + " " + release.version + " was imported." });
    });
  }

  function confirm() {
    const choice = prompt;
    setPrompt(null);
    if (!choice) return;
    if (choice.type === "select") {
      useInstallation(choice.candidate);
      return;
    }
    void run("Applying setup", async () => {
      const release = choice.release;
      const status = await api.apply(release.id, release.version);
      updateStatus(status);
      setFeedback({ kind: status.packageId === release.id && status.version === release.version ? "info" : "error",
        text: status.name + (status.version ? " " + status.version : "") + " \u00b7 file check complete." });
    });
  }

  function recover() {
    void run("Recovering files", async () => {
      updateStatus(await api.recover());
      setFeedback({ kind: "info", text: "The previous setup has been restored." });
    });
  }

  const launchButton = (label = "Launch CoD2x") => <button className="button button-primary" disabled={!verified || Boolean(busy)} onClick={launch}>
    <Play size={16} fill="currentColor" /> {label}
  </button>;

  const setupButton = (release: Package) => {
    const active = installation?.packageId === release.id && installation.version === release.version;
    return active ? launchButton(release.kind === "official" ? "Launch CoD2x" : "Launch " + release.name)
      : <button className="button button-primary" disabled={Boolean(busy) || installation?.recoveryPending} onClick={() => startSetup(release)}>
        <ArrowDownToLine size={16} /> {installation ? "Set up version" : "Choose installation"}
      </button>;
  };

  function renderPackage(release: Package) {
    return <>
      <div className="mod-body">
        <div><h3>Release notes</h3><p className="release-text">{release.releaseNotes || "No release notes were included."}</p></div>
        <div><h3>Package files</h3><ul className="file-lines">{release.files.map((file) => <li key={file.path}><Check size={14} /><code>{file.path}</code></li>)}</ul></div>
      </div>
      <div className="panel-footer">
        <p>Requires official CoD2x {release.baseVersion}</p>
        {setupButton(release)}
      </div>
    </>;
  }

  function onboarding() {
    const candidates = data.candidates.slice();
    if (onboardingFolder && !candidates.some((item) => item.path === onboardingFolder.path)) {
      candidates.push(onboardingFolder);
    }
    const selected = onboardingFolder ?? candidates[0];
    return <main className="onboarding">
      <div className="onboarding-frame">
        <div className="onboarding-art" aria-hidden="true"><img src="/launcher-emblem.png" alt="" /></div>
        <div className="onboarding-content">
          <div className="tile-icon"><FolderOpen size={22} /></div>
          <h1>Choose your CoD2 installation</h1>
          <p>Confirm the folder you play from. The launcher will verify the game files and identify the setup before you continue.</p>
          {loadingInstallation ? <p className="onboarding-scan"><RefreshCw className="spin" size={16} /> Looking for Steam and retail installs</p>
            : candidates.length > 0 ? <fieldset className="folder-options">
              <legend>Installation folder</legend>
              {candidates.map((candidate) => <label className="folder-option" key={candidate.path}>
                <input type="radio" name="install-folder" checked={candidate.path === selected?.path} onChange={() => setOnboardingFolder(candidate)} disabled={Boolean(busy)} />
                <span><strong>{candidate.source}</strong><code>{candidate.path}</code></span>
              </label>)}
            </fieldset> : <p className="onboarding-scan">Select the folder that contains CoD2MP_s.exe.</p>}
          {feedback?.kind === "error" && <p className="onboarding-error" role="alert">{feedback.text}</p>}
          <div className="actions onboarding-actions">
            <button className="button button-secondary" onClick={() => choose()} disabled={Boolean(busy) || loadingInstallation}><FolderOpen size={16} /> Browse folders</button>
            <button className="button button-primary" disabled={!selected || Boolean(busy) || loadingInstallation} onClick={() => selected && useInstallation(selected)}><ShieldCheck size={17} /> Verify and continue</button>
          </div>
        </div>
      </div>
    </main>;
  }

  function overview() {
    if (!installation) return null;
    const status = installation;
    const canRestore = status.kind !== "official" && !status.recoveryPending;
    const [id, version] = chosenKey.split("/");
    const packageToSetUp = customPackages.get(chosenKey);
    const isHosted = hosted.some((item) => setupKey(item.manifest.id, item.manifest.version) === chosenKey);
    const settingOfficial = id === OFFICIAL;
    const selectedIsActive = chosenKey === activeKey && Boolean(verified);
    const selectedIsRestore = canRestore && settingOfficial && version === restoreVersion;

    function setupSelected() {
      if (settingOfficial) {
        setupOfficial(version);
      } else if (packageToSetUp) {
        if (isHosted) prepareMod(packageToSetUp);
        else startSetup(packageToSetUp);
      }
    }

    const activeDescription = status.packageId && status.version
      ? status.name + " " + status.version : status.officialVersion
        ? "CoD2x " + status.officialVersion + " detected" : null;

    return <>
      <section className="hero overview-hero" aria-labelledby="launch-heading">
        <div className="hero-art" aria-hidden="true"><img src="/launcher-emblem.png" alt="" /></div>
        <div className="hero-content">
          <h1 id="launch-heading">CALL OF DUTY 2</h1>
          <div className="launch-state">
            <StatusPill status={status} />
            {activeDescription && <span>{activeDescription}</span>}
            {latestVerified && status.kind !== "mixed" && (status.kind !== "official" || status.version !== latest.version) &&
              <button className="text-action" onClick={() => {
                setVersions((previous) => ({ ...previous, official: latest.version }));
                go("official");
              }}>Latest official {latest.version} <ArrowRight size={13} /></button>}
          </div>
          <label className="setup-label" htmlFor="overview-setup">Setup to launch</label>
          <div className="actions setup-actions">
            <Select id="overview-setup" value={chosenKey} onChange={setSetupChoice} disabled={Boolean(busy) || status.recoveryPending}>
              <optgroup label="Official CoD2x">
                {officialVersions.map((entry) => <option key={entry} value={setupKey(OFFICIAL, entry)}>
                  {"CoD2x " + entry + (entry === latest.version && latestVerified ? " (latest)" : "")}
                </option>)}
              </optgroup>
              {customPackages.size > 0 && <optgroup label="Mods and local builds">
                {[...customPackages].map(([key, item]) => <option key={key} value={key}>{item.name + " " + item.version}</option>)}
              </optgroup>}
            </Select>
            {status.recoveryPending
              ? <button className="button button-primary" disabled={Boolean(busy)} onClick={recover}><RotateCcw size={16} /> Recover previous files</button>
              : selectedIsRestore
                ? <button className="button button-primary" disabled={Boolean(busy)} onClick={restoreOfficial}><RotateCcw size={16} /> Restore Official CoD2x</button>
                : selectedIsActive
                  ? launchButton(settingOfficial ? "Play CoD2x" : "Play " + status.name)
                  : <button className="button button-primary" disabled={Boolean(busy)} onClick={setupSelected}><ArrowDownToLine size={16} /> Set up selected version</button>}
          </div>
          {(selectedIsRestore || status.recoveryPending) && <p className="setup-hint">
            {status.recoveryPending ? "Recover the previous setup before launching."
              : "Backs up managed mod files and verifies official CoD2x " + restoreVersion + "."}
          </p>}
          <div className="actions launch-links">
            <button className="button button-secondary hero-secondary" onClick={() => go("mods")} disabled={Boolean(busy)}><Layers3 size={16} /> Set up a different mod <ArrowRight size={15} /></button>
            {canRestore && !selectedIsRestore && <button className="text-action" disabled={Boolean(busy)} onClick={restoreOfficial}><RotateCcw size={13} /> Restore Official CoD2x</button>}
          </div>
        </div>
      </section>
      <section className="install-strip" aria-label="Game installation">
        <div className="tile-icon strip-icon"><HardDrive size={19} /></div>
        <div className="install-text"><h3>Game folder</h3><span title={status.path}>{status.path}</span></div>
        <button className="button button-secondary button-small" onClick={() => go("installation")} disabled={Boolean(busy)}><FolderOpen size={15} /> Manage installation</button>
      </section>
    </>;
  }

  function official() {
    const active = installation?.packageId === OFFICIAL && installation.version === selectedRelease.version;
    return <>
      <PageTitle title="Official CoD2x" description="Windows client releases from the upstream project." />
      <section className="panel">
        <div className="panel-heading package-heading">
          <div className="tile-icon"><ShieldCheck size={23} /></div>
          <div><h2>{selectedRelease.title}</h2><p>Yctn and eyza <span className="middot">&middot;</span> {releaseDate(selectedRelease.publishedAt)}</p></div>
          {active && <span className="status-pill status-good"><span className="status-dot" /> Active</span>}
        </div>
        <div className="selection-row">
          <label htmlFor="official-version">Release version</label>
          <Select id="official-version" value={selectedRelease.version} onChange={(version) => setVersions({ ...versions, official: version })}>
            {releases.map((item, index) => <option value={item.version} key={item.version}>{item.version}{index === 0 && latestVerified ? " (latest stable)" : ""}</option>)}
          </Select>
          <span className="source-label"><Github size={15} /> {releaseMessage}</span>
        </div>
        <div className="release-notes">
          <h3>Release notes</h3>
          <div className="release-text">{releaseSummary(selectedRelease.notes).join("\n") || "No release notes were supplied."}</div>
        </div>
        <div className="panel-footer">
          <p>{(selectedRelease.downloadSize / 1000000).toFixed(1)} MB <span className="middot">&middot;</span> Windows ZIP <span className="middot">&middot;</span> SHA-256</p>
          <div className="actions">
            {!selectedOfficial && !active && <button className="button button-secondary" disabled={Boolean(busy)} onClick={() => void run("Downloading release", async () => {
              await download(selectedRelease.version);
              setFeedback({ kind: "info", text: "CoD2x " + selectedRelease.version + " was downloaded and verified." });
            })}><ArrowDownToLine size={16} /> Download</button>}
            {active ? launchButton()
              : selectedOfficial ? setupButton(selectedOfficial)
              : <button className="button button-primary" disabled={Boolean(busy) || installation?.recoveryPending} onClick={() => setupOfficial(selectedRelease.version)}><ShieldCheck size={16} /> {installation ? "Set up official release" : "Choose installation"}</button>}
          </div>
        </div>
      </section>
    </>;
  }

  function mods() {
    const groups = new Map<string, { manifest: Package; download?: HostedRelease }[]>([[PLAYTEST, []]]);
    for (const download of hosted) {
      const id = download.manifest.id;
      const releases = groups.get(id) ?? [];
      releases.push({ manifest: download.manifest, download });
      groups.set(id, releases);
    }
    const playtest = groups.get(PLAYTEST)!;
    for (const manifest of local.filter((item) => item.id === PLAYTEST)) {
      if (!playtest.some((item) => item.manifest.version === manifest.version)) playtest.push({ manifest });
    }

    return <>
      <PageTitle title="Other Mods" description="Choose a community mod and install a release for your game." action={<button className="button button-secondary" onClick={() => go("publish")}><PackagePlus size={16} /> Publish a mod</button>} />
      {catalogError && <p className="catalog-error" role="status">Mods catalog unavailable. {catalogError}</p>}
      <div className="panel mods-list" aria-label="Community mod catalog">
        <div className="mod-list-head mod-list-grid" aria-hidden="true">
          <span>Mod</span><span>Author</span><span>Version</span><span>Setup</span>
        </div>
        <ul aria-label="Available mods">
          {[...groups].map(([id, releases]) => {
            const sorted = releases.sort((a, b) => b.manifest.version.localeCompare(a.manifest.version, "en", { numeric: true }));
            const preferred = modVersions[id] ?? (installation?.packageId === id ? installation.version : undefined);
            const selected = sorted.find(({ manifest }) => manifest.version === preferred) ?? sorted[0];
            const release = selected?.manifest;
            const name = release?.name ?? "125 Hz Playtest";
            const cached = release && cachedMods.some((item) => sameManifest(item, release));
            const current = Boolean(release && cached && installation?.packageId === id && installation.version === release.version
              && release.files.every((file) => installation.files.some((actual) => actual.path === file.path && actual.sha256 === file.sha256)));
            const disabled = Boolean(busy) || installation?.recoveryPending;
            return <li className="mod-row mod-list-grid" key={id} aria-label={name}>
              <div className="mod-name">
                <div className="tile-icon" aria-hidden="true"><Gamepad2 size={19} /></div>
                <div>
                  <h2>{name}</h2>
                  <p>{release?.description || "Claude2 client and zPAM for the 125 Hz playtest."}</p>
                  {release && <details className="mod-release-details">
                    <summary>Release details</summary>
                    <p>Requires CoD2x {release.baseVersion}{selected.download ? " \u00b7 " + (selected.download.size / 1_000_000).toFixed(1) + " MB" : ""}</p>
                    {release.releaseNotes && <p className="release-text">{release.releaseNotes}</p>}
                  </details>}
                </div>
              </div>
              <div className="mod-author">{release?.authors.length ? release.authors.join(", ") : "Unlisted"}</div>
              <div className="mod-version">
                {release ? <Select id={"version-" + id} label={name + " version"} value={release.version} disabled={Boolean(busy)} onChange={(version) => setModVersions((previous) => ({ ...previous, [id]: version }))}>
                  {sorted.map(({ manifest }) => <option value={manifest.version} key={manifest.version}>{manifest.version}</option>)}
                </Select> : <span>Awaiting release</span>}
              </div>
              <div className="mod-action">
                {current ? <span className="status-pill status-good"><Check size={13} /> Current</span>
                  : !release ? <button className="button button-secondary button-small" disabled={Boolean(busy)} onClick={importZip}>Import ZIP</button>
                  : <button className="button button-primary button-small" disabled={disabled} onClick={() => {
                    if (selected.download) prepareMod(release);
                    else startSetup(release);
                  }}>{cached ? "Set up" : "Install"}</button>}
              </div>
            </li>;
          })}
        </ul>
      </div>
    </>;
  }

  function localBuilds() {
    return <>
      <PageTitle title="Local Builds" description="Import and set up packages from your own build output."
        action={<button className="button button-primary" disabled={Boolean(busy)} onClick={importZip}><PackagePlus size={16} /> Import ZIP</button>} />
      {selectedLocal ? <section className="panel">
        <div className="panel-heading package-heading"><div className="tile-icon"><Box size={23} /></div><div><h2>{selectedLocal.name}</h2><p>{selectedLocal.authors.join(" / ") || "Local build"}</p></div></div>
        <p className="package-description">{selectedLocal.description}</p>
        <div className="selection-row"><label htmlFor="local-version">Imported release</label><Select id="local-version" value={selectedLocal.id + "/" + selectedLocal.version} onChange={(version) => setVersions({ ...versions, local: version })}>{local.map((release) => <option key={release.id + "/" + release.version} value={release.id + "/" + release.version}>{release.name} {release.version}</option>)}</Select></div>
        {renderPackage(selectedLocal)}
      </section> : <EmptyState icon={Box} title="No local builds yet">
        <p>Import a ZIP containing its manifest and package files.</p>
        <button className="button button-secondary" disabled={Boolean(busy)} onClick={importZip}><PackagePlus size={16} /> Import a build</button>
      </EmptyState>}
    </>;
  }

  function installations() {
    return <>
      <PageTitle title="Installation" description="Choose the Call of Duty 2 folder this launcher manages."
        action={<button className="button button-secondary" onClick={() => choose()} disabled={Boolean(busy)}><FolderOpen size={17} /> Browse folders</button>} />
      {installation && <section className="panel installation-panel">
        <div className="panel-heading package-heading">
          <div className="tile-icon"><HardDrive size={22} /></div>
          <div className="wide-heading"><h2>{installation.name}{installation.version ? " " + installation.version : ""}</h2><p className="path-text">{installation.path}</p></div>
          <StatusPill status={installation} />
        </div>
        <details className="file-details">
          <summary>File details</summary>
          {installation.notes.length > 0 && <ul className="issue-list">{installation.notes.map((note) => <li key={note}><CircleAlert size={16} /> {note}</li>)}</ul>}
          <div className="tracked-files">
            <p>Verification covers the official DLLs and mod files managed by this launcher.</p>
            {installation.files.map((file) => <div className="tracked-file" key={file.path}><code>{file.path}</code><span title={file.sha256 ?? undefined}>{file.sha256 ? file.sha256.slice(0, 12) + "\u2026" : "Absent"}</span></div>)}
          </div>
        </details>
        <div className="panel-footer">
          <p>{installation.executableVerified ? "Recognized Steam executable" : "Game layout detected"}</p>
          <div className="actions">
            <button className="button button-secondary" onClick={verify} disabled={Boolean(busy)}><RefreshCw size={16} /> Verify files</button>
            {installation.recoveryPending ? <button className="button button-primary" onClick={recover} disabled={Boolean(busy)}><RotateCcw size={16} /> Recover previous files</button>
              : installation.kind !== "official" && <button className="button button-primary" onClick={restoreOfficial} disabled={Boolean(busy)}><RotateCcw size={16} /> Restore Official CoD2x</button>}
          </div>
        </div>
      </section>}
      <section className="panel candidate-list">
        <div className="list-heading"><h2>Detected installations</h2>
          <button className="button button-secondary button-small" disabled={Boolean(busy)} onClick={() => void run("Searching for CoD2", async () => {
            const candidates = await api.discover();
            setData((previous) => ({ ...previous, candidates }));
          })}><RefreshCw size={14} /> Rescan</button>
        </div>
        {data.candidates.length ? data.candidates.map((candidate) => <button className="candidate" key={candidate.path} onClick={() => choose(candidate)} disabled={Boolean(busy)}>
          <Folder size={18} /><div><strong>{candidate.source}</strong><span>{candidate.path}</span></div><ChevronRight size={17} />
        </button>) : <p className="list-empty">No installation was detected. Browse to the folder containing CoD2MP_s.exe.</p>}
      </section>
    </>;
  }

  return <div className="app-shell">
    <header className="titlebar" data-tauri-drag-region>
      <div className="titlebar-brand" data-tauri-drag-region><img src="/launcher-emblem.png" alt="" /><span>CoD2x Launcher</span></div>
      {!native && <span className="preview-label">Browser preview</span>}
      <AppWindowControls />
    </header>
    {!installation ? onboarding() : <div className="workspace">
      <aside className="sidebar">
        <nav aria-label="Launcher pages">
          {navigation.map(({ id, label, icon: Icon }) => <button key={id} aria-label={label} className={"nav-item " + (page === id ? "nav-current" : "")} aria-current={page === id ? "page" : undefined} onClick={() => go(id)}><Icon size={18} /><span>{label}</span></button>)}
        </nav>
        <div className="sidebar-bottom">
          <button aria-label="Installation" className={"nav-item " + (page === "installation" ? "nav-current" : "")} aria-current={page === "installation" ? "page" : undefined} onClick={() => go("installation")}><FolderOpen size={18} /><span>Installation</span></button>
          <div className="sidebar-meta"><span className="status-dot" /> Windows <span>v0.1.0</span></div>
        </div>
      </aside>
      <main className="main-view" key={page}>
        <div className="page-content">
          {page === "overview" && overview()}
          {page === "official" && official()}
          {page === "mods" && mods()}
          {page === "publish" && <PublishView baseVersion={latest.version} baseVersions={officialVersions} onBack={() => go("mods")} />}
          {page === "local" && localBuilds()}
          {page === "installation" && installations()}
        </div>
      </main>
    </div>}
    {(busy || (feedback && installation)) && <div className={"toast " + (!busy && feedback?.kind === "error" ? "toast-error" : "")} role={!busy && feedback?.kind === "error" ? "alert" : "status"}>
      {busy ? <RefreshCw className="spin" size={17} /> : feedback?.kind === "error" ? <CircleAlert size={17} /> : <CircleCheck size={17} />}
      <span>{busy || feedback?.text}</span>
      {!busy && <button className="icon-button" onClick={() => setFeedback(null)} aria-label="Dismiss message"><X size={16} /></button>}
    </div>}
    {prompt && <Modal title={prompt.type === "select" ? "Confirm installation" : "Apply " + prompt.release.name} onClose={() => setPrompt(null)}>
      <p>{prompt.type === "select"
        ? "The launcher will inspect this folder and use it for future setups."
        : "CoD2 must be closed. Replaced files and managed archives will be backed up outside the game folder before the new setup is applied."}</p>
      <code className="dialog-path">{prompt.type === "select" ? prompt.candidate.path : installation?.path}</code>
      {prompt.type === "apply" && <>
        <p>Selected version <strong>{prompt.release.version}</strong></p>
        {installation?.extraManagedFiles.some((file) => !prompt.release.files.some((item) => item.path === file)) &&
          <div className="backup-preview">
            <strong>Moved out of the game folder</strong>
            <ul>{installation.extraManagedFiles.filter((file) => !prompt.release.files.some((item) => item.path === file)).map((file) => <li key={file}><code>{file}</code></li>)}</ul>
          </div>}
      </>}
      <div className="dialog-actions"><button className="button button-secondary" onClick={() => setPrompt(null)}>Cancel</button><button className="button button-primary" onClick={confirm}><ClipboardCheck size={16} />{prompt.type === "select" ? "Use this folder" : "Apply setup"}</button></div>
    </Modal>}
  </div>;
}

function Select({ id, value, onChange, children, disabled = false, label }: { id: string; value: string; onChange: (value: string) => void; children: ReactNode; disabled?: boolean; label?: string }) {
  return <div className="select-wrap"><select id={id} aria-label={label} value={value} disabled={disabled} onChange={(event) => onChange(event.target.value)}>{children}</select><ChevronDown size={15} aria-hidden="true" /></div>;
}
