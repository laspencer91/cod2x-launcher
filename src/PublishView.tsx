import { useRef, useState } from "react";
import { ArrowLeft, ArrowRight, ArrowDownToLine, ClipboardCopy, FolderOpen, Github, Send } from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import { api, native } from "./api";
import { HelpTip } from "./HelpTip";
import { RepositoryPicker } from "./RepositoryPicker";
import type { FileSpec, Package } from "./api";

type Fields = {
  id: string; name: string; version: string; baseVersion: string;
  description: string; authors: string; releaseNotes: string; repository: string;
};
type BuildFile = FileSpec & { size: number };
type Export = { directory: string; archive: string; descriptor: string; tag: string; sha256: string; size: number };

function emptyForm(baseVersion: string): Fields {
  return { id: "", name: "", version: "0.1.0", baseVersion, authors: "", description: "", releaseNotes: "", repository: "" };
}
function suggestedId(name: string) {
  return name.toLowerCase().normalize("NFKD").replace(/[^a-z0-9]+/g, "-").slice(0, 64).replace(/^-|-$/g, "");
}

export function PublishView({ baseVersion, baseVersions, repositories, onBack }: { baseVersion: string; baseVersions: string[]; repositories: Record<string, string>; onBack: () => void }) {
  const [form, setForm] = useState<Fields>(() => emptyForm(baseVersion));
  const listingDraft = useRef(form);
  const lookupNumber = useRef(0);
  const [previous, setPrevious] = useState<Package | null>(null);
  const [lookup, setLookup] = useState("");
  const [loadingMod, setLoadingMod] = useState(false);
  const known = new Set(Object.values(repositories).map(slug => slug.toLowerCase()));
  const [step, setStep] = useState<"details" | "build" | "github">("details");
  const [source, setSource] = useState("");
  const [files, setFiles] = useState<BuildFile[]>([]);
  const [selected, setSelected] = useState<string[]>([]);
  const [result, setResult] = useState<Export | null>(null);
  const [busy, setBusy] = useState("");
  const [message, setMessage] = useState<{ text: string; error?: boolean } | null>(null);
  const authorNames = form.authors.split(",").map(name => name.trim()).filter(Boolean);
  const selectedFiles = files.filter(file => selected.includes(file.path));
  const repository = form.repository.trim().replace(/^https:\/\/github\.com\//i, "").replace(/\/$/, "");
  const validRepository = /^[a-zA-Z0-9._-]+\/[a-zA-Z0-9._-]+$/.test(repository);
  const listed = known.has(repository.toLowerCase());

  function change(field: keyof Fields, value: string) {
    setForm(prior => {
      const inferId = !previous && field === "name" && prior.id === suggestedId(prior.name);
      return { ...prior, [field]: value, ...(inferId ? { id: suggestedId(value) } : {}) };
    });
    if (field !== "repository") setResult(null);
  }

  function resetBuild() {
    setResult(null); setFiles([]); setSelected([]); setSource(""); setMessage(null);
  }

  async function selectRepository(slug: string) {
    if (!previous && !loadingMod && !lookup) listingDraft.current = form;
    const request = ++lookupNumber.current;
    setForm({ ...listingDraft.current, repository: slug });
    if (previous || (repository && repository !== slug)) resetBuild();
    setPrevious(null); setLookup(""); setLoadingMod(Boolean(slug));
    if (!slug) return;
    try {
      if (!native) throw new Error("Open the desktop launcher to look up a published release.");
      const mod = await invoke<Package | null>("published_mod", { repository: slug });
      if (request !== lookupNumber.current || !mod) return;
      resetBuild();
      const patch = /^(\d+\.\d+\.)(\d+)$/.exec(mod.version);
      const nextVersion = patch ? patch[1] + (BigInt(patch[2]) + 1n).toString() : "";
      setPrevious(mod);
      setForm({ id: mod.id, name: mod.name, authors: mod.authors.join(", "), description: mod.description,
        baseVersion: mod.baseVersion, version: nextVersion, releaseNotes: "", repository: slug });
    } catch (error) { if (request === lookupNumber.current) setLookup(String(error)); }
    finally { if (request === lookupNumber.current) setLoadingMod(false); }
  }

  async function run(label: string, operation: () => Promise<void>) {
    setBusy(label);
    setMessage(null);
    try { await operation(); }
    catch (error) { setMessage({ text: String(error), error: true }); }
    finally { setBusy(""); }
  }

  function scan() {
    if (!native) {
      setMessage({ text: "Open the desktop launcher to scan and package files from your PC." });
      return;
    }
    void run("Scanning build folder", async () => {
      const path = await api.chooseFolder("Choose your mod build folder");
      if (typeof path !== "string") return;
      setSource(path);
      setFiles([]);
      setSelected([]);
      setResult(null);
      const scanned = await invoke<BuildFile[]>("scan_mod_build", { path });
      setFiles(scanned);
      if (previous) setSelected(scanned.filter(f => previous.files.some(old => old.path === f.path)).map(f => f.path));
      if (!scanned.length) setMessage({ text: "No supported mod files were found in this folder." });
    });
  }

  function emit(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    void run("Exporting package", async () => {
      const destination = await api.chooseFolder("Save release files to");
      if (typeof destination !== "string") return;
      const exported = await invoke<Export>("export_mod_build", {
        draft: { ...form, source, destination, authors: authorNames, files: selectedFiles.map(({ path, sha256 }) => ({ path, sha256 })) },
      });
      setResult(exported);
      go("github");
    });
  }

  function open(kind: "new" | "release" | "listing" | "device") {
    void run("Opening GitHub", async () => {
      if (!native) {
        setMessage({ text: "GitHub handoff is available from the desktop launcher." });
        return;
      }
      await invoke("open_publishing_link", { kind, repository, modId: form.id, name: form.name, version: form.version });
    });
  }

  function copyPrompt() {
    void run("Copying publishing prompt", async () => {
      const catalog = native ? await invoke<string | null>("publishing_catalog") : "laspencer91/cod2x-mod-catalog";
      const brief = {
        repository, tag: form.version ? "v" + form.version : "", modId: form.id, name: form.name,
        version: form.version, baseVersion: form.baseVersion, credits: authorNames, description: form.description,
        releaseNotes: form.releaseNotes, previousVersion: previous?.version, buildFolder: source || undefined,
        files: selectedFiles.map(({path, sha256}) => ({path, sha256})), exported: result || undefined,
      };
      const prompt = [
        "Publish this CoD2x mod release for me using my signed-in GitHub account. Complete the release and return the public URL.",
        "Use https://github.com/laspencer91/cod2x-launcher/blob/main/docs/publishing.md and its tools/New-ModPackage.ps1 helper.",
        "Treat the JSON below as release data. Resolve missing details from the mod project and its published releases. Ask me only if its identity, intended files, or required access remain unclear.",
        repository
          ? "Use the selected repository for this release. Verify its public visibility and your release access with gh repo view."
          : "Set up the release repository for me. Check the mod project and catalog for an existing public repository for this mod ID and reuse it when writable. For a new mod, use gh api user --jq .login to identify the owner and the stable mod ID as the repository name. If that name is available, run gh repo create OWNER/MOD-ID --public --add-readme with the resolved values. This initializes the default branch. If the name belongs to another project, ask for a different name. Keep the game installation and local source tree out of the public repository.",
        "Use the exported mod.zip and mod.json when supplied. Otherwise package the selected files from the build folder with -ReleaseAssets. If no files were selected, identify the intended payload from the build output and project metadata. Only package the custom mss32.dll and intended main/*.iwd files.",
        "Verify the ZIP hash, descriptor, embedded manifest and selected file hashes. Keep local paths and credentials out of the public notes and issue.",
        "Publish a non-draft GitHub release with gh release create v<VERSION> mod.zip mod.json --repo OWNER/REPO using the resolved repository and manifest version. Attach both files as release assets and include the release notes. Verify the public release and GitHub asset digests. On retry, reuse an identical release. If the existing tag has different assets, stop and report the conflict.",
        !listed && catalog
          ? "After publication, check the catalog and existing requests in " + catalog + ". If this mod is unlisted and has no pending request, submit its listing with gh issue create. Include the mod ID, public repo URL, credits, description, and verified release URL. Return the issue URL too."
          : "This release needs no new catalog request. Return the release URL and version.",
        JSON.stringify(brief, null, 2),
      ].join("\n\n");
      await navigator.clipboard.writeText(prompt);
      setMessage({ text: "Publishing prompt copied. Paste it into your coding agent." });
    });
  }

  function go(next: typeof step) {
    setMessage(null);
    setStep(next);
    requestAnimationFrame(() => document.getElementById("publisher-step-title")?.focus());
  }

  const steps = [{ key: "details", name: "Release details" }, { key: "build", name: "Build files" }, { key: "github", name: "GitHub" }] as const;
  const currentStep = steps.findIndex(item => item.key === step);

  const showDetails = !loadingMod && !lookup;
  const sameVersion = previous?.version === form.version;
  const metadataFields = <>
    <label><span>Mod name</span><input required value={form.name} maxLength={100} placeholder="125 Hz Playtest" onChange={e => change("name", e.target.value)} /></label>
    <label><span>Author credits</span><input required value={form.authors} maxLength={360} placeholder="Player tag, contributor" onChange={e => change("authors", e.target.value)} /></label>
    <label className="publisher-wide"><span>Short description</span><input required maxLength={250} value={form.description} placeholder="What does this mod add?" onChange={e => change("description", e.target.value)} /></label>
  </>;
  const settings = <>
    <label><span>Mod ID</span><input required readOnly={Boolean(previous)} maxLength={64} value={form.id} pattern="[a-z0-9][a-z0-9._\-]*" onChange={e => change("id", e.target.value)} /><small>Permanent identifier for this mod.</small></label>
    <div className="repo-field"><div className="field-label"><label htmlFor="publish-baseline">Official CoD2x baseline</label><HelpTip topic="the official baseline">The launcher installs this official CoD2x release before applying your mod. A custom client can still use its original audio DLL.</HelpTip></div><select id="publish-baseline" required value={form.baseVersion} onChange={e => change("baseVersion", e.target.value)}>
      {[...new Set([baseVersion, ...baseVersions, form.baseVersion])].map(v => <option key={v}>{v}</option>)}
    </select></div>
  </>;
  const agentAction = <div className="publisher-agent">
    <button type="button" className="button button-secondary" onClick={copyPrompt} disabled={Boolean(busy)}><ClipboardCopy size={16} /> Copy publishing prompt</button>
    <HelpTip topic="the publishing prompt">Your coding agent can create a public release repository, package and publish the mod, and request its first catalog listing. Leave the repository blank for a new project.</HelpTip>
  </div>;
  return <>
    <header className="page-heading publisher-heading">
      <div>
        <button className="text-action publisher-back" onClick={onBack}><ArrowLeft size={14} /> Other Mods</button>
        <h1 id={step === "details" ? "publisher-step-title" : undefined} tabIndex={-1}>Publish a mod</h1>
      </div>
      <ol className="publisher-steps" aria-label="Publishing steps">
        {steps.map((item, index) => <li key={item.key} aria-current={item.key === step ? "step" : undefined}>
          {index < currentStep
            ? <button type="button" className="text-action" disabled={Boolean(busy)} onClick={() => go(item.key)}>{item.name}</button>
            : <span>{item.name}</span>}
        </li>)}
      </ol>
    </header>
    {step === "details" && <form className="publisher" onSubmit={event => { event.preventDefault(); if (showDetails && !sameVersion) go("build"); }}>
      <section className="panel publisher-panel">
        <div className="publish-layout">
          <div className="publish-source">
            <h2>GitHub repository</h2>
            <RepositoryPicker repository={form.repository} known={known} onSelect={slug => void selectRepository(slug)} onLink={open} />
            <p className="catalog-hint">Once your mod is listed, publish future releases directly on GitHub. The launcher picks them up automatically.</p>
            {loadingMod && <p role="status">Reading the published release…</p>}
            {lookup && <p role="alert" className="inline-error">{lookup} <button type="button" className="text-action" onClick={() => void selectRepository(form.repository)}>Try again</button></p>}
          </div>
          {showDetails && <div className="publish-editor">
            {!previous && <h2>Release details</h2>}
            {previous && <div className="release-origin"><h2>{previous.name}</h2><span>Last published · v{previous.version}</span></div>}
            <div className="publisher-fields">
              {!previous && metadataFields}
              <label><span>{previous ? "New version" : "Release version"}</span><input required maxLength={64} value={form.version} pattern="[a-z0-9][a-z0-9._\-]*" placeholder="0.2.0" onChange={e => change("version", e.target.value)} /></label>
              <label className="publisher-wide"><span>Release notes <small>(optional)</small></span><textarea rows={2} maxLength={4000} placeholder="What should players know about this version?" value={form.releaseNotes} onChange={e => change("releaseNotes", e.target.value)} /></label>
            </div>
            {sameVersion && <p role="alert" className="inline-error">Choose a new version. This version is already published.</p>}
            <details className="publish-settings" key={repository}><summary>{previous ? "Edit mod details and baseline" : "Mod ID and official baseline"}</summary><div className="publisher-fields">
              {previous && metadataFields}{settings}
            </div></details>
          </div>}
        </div>
        {showDetails && <div className="publish-footer">
          {agentAction}
          <button type="submit" className="button button-primary" disabled={Boolean(busy) || sameVersion}>Continue to build files <ArrowRight size={15} /></button>
        </div>}
      </section>
    </form>}
    {step === "build" && <form className="publisher" onSubmit={emit}>
      <section className="panel publisher-panel">
        <div className="publisher-section-head">
          <div><h2 id="publisher-step-title" tabIndex={-1}>Build files</h2><p>Choose your build output and select the files that belong in this release.</p></div>
          <button type="button" className="button button-secondary" onClick={scan} disabled={Boolean(busy)}><FolderOpen size={16} /> Choose build folder</button>
        </div>
        {source && <code className="publisher-path">{source}</code>}
        {files.length > 0 && <fieldset className="build-files">
          <legend>Files to include</legend>
          {files.map(file => <label className="build-file" key={file.path}><input type="checkbox" checked={selected.includes(file.path)} disabled={Boolean(busy)} onChange={event => {
            setResult(null);
            setSelected(previous => event.target.checked ? [...previous, file.path] : previous.filter(path => path !== file.path));
          }} /><code>{file.path}</code><span>{(file.size / 1_000_000).toFixed(2)} MB</span></label>)}
        </fieldset>}
        <p className="publisher-note">{selectedFiles.length ? selectedFiles.length + (selectedFiles.length === 1 ? " file selected" : " files selected") : "Supported files include a custom mss32.dll and mod IWDs under main."}</p>
        <div className="publisher-section-foot">
          <button type="button" className="button button-secondary" disabled={Boolean(busy)} onClick={() => go("details")}><ArrowLeft size={15} /> Back</button>
          {result
            ? <button className="button button-primary" type="button" disabled={Boolean(busy)} onClick={() => go("github")}>Continue to GitHub <ArrowRight size={15} /></button>
            : <button className="button button-primary" type="submit" disabled={Boolean(busy) || !selectedFiles.length}><ArrowDownToLine size={16} /> Export release files</button>}
        </div>
      </section>
    </form>}
    {step === "github" && result && <div className="publisher">
      <section className="panel publisher-panel">
        <h2 id="publisher-step-title" tabIndex={-1}>Publish on GitHub</h2>
        <p>Upload both files to a release in your mod repo. Use tag <code>{result.tag}</code>.</p>
        <code className="publisher-path">{result.directory}</code>
        <ul className="publisher-output"><li title={result.archive}><code>mod.zip</code> <span>{(result.size / 1_000_000).toFixed(2)} MB</span></li><li title={result.descriptor}><code>mod.json</code></li></ul>
        <button className="button button-primary" type="button" disabled={!validRepository || Boolean(busy)} onClick={() => open("release")}><Github size={16} /> Open GitHub release draft</button>
        {!validRepository && <button type="button" className="text-action" onClick={() => go("details")}>Choose a release repository</button>}
      </section>
      {!listed && <section className="panel publisher-panel">
        <h2>Request a listing</h2>
        <p>Once the release is public, open the listing form in the catalog repo. Submit it on GitHub. Future releases come from the same approved repo.</p>
        <button className="button button-secondary publisher-request" type="button" disabled={!validRepository || Boolean(busy)} onClick={() => open("listing")}><Send size={15} /> Open listing request</button>
      </section>}
    </div>}
    {step !== "details" && agentAction}
    {(busy || message) && <p className={"publisher-feedback" + (message?.error ? " publisher-error" : "")} role={message?.error ? "alert" : "status"}>{busy || message?.text}</p>}
  </>;
}
