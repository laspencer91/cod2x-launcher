import { useState } from "react";
import { ArrowLeft, ArrowRight, ArrowDownToLine, ClipboardCopy, FolderOpen, Github, Send } from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import { api, native } from "./api";
import type { FileSpec } from "./api";

type Fields = {
  id: string; name: string; version: string; baseVersion: string;
  description: string; authors: string; releaseNotes: string; repository: string;
};
type BuildFile = FileSpec & { size: number };
type Export = { directory: string; archive: string; descriptor: string; tag: string; sha256: string; size: number };

const fields: { field: keyof Fields; label: string; placeholder: string; max: number; required?: boolean }[] = [
  { field: "name", label: "Mod name", placeholder: "125 Hz Playtest", max: 100, required: true },
  { field: "id", label: "Mod ID", placeholder: "cod2x-claude2", max: 64, required: true },
  { field: "version", label: "Version", placeholder: "0.1.0", max: 64, required: true },
  { field: "authors", label: "Authors (comma-separated)", placeholder: "Author, Contributor", max: 360, required: true },
  { field: "repository", label: "Public GitHub mod repo", placeholder: "owner/my-mod", max: 201 },
];

export function PublishView({ baseVersion, baseVersions, onBack }: { baseVersion: string; baseVersions: string[]; onBack: () => void }) {
  const [form, setForm] = useState<Fields>({ id: "", name: "", version: "", baseVersion, description: "", authors: "", releaseNotes: "", repository: "" });
  const [step, setStep] = useState<"details" | "build" | "github">("details");
  const [mode, setMode] = useState<"listing" | "update">("listing");
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

  function change(field: keyof Fields, value: string) {
    setForm(previous => ({ ...previous, [field]: value }));
    if (field !== "repository") setResult(null);
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

  function open(kind: "new" | "release" | "listing") {
    void run("Opening GitHub", async () => {
      if (!native) {
        setMessage({ text: "GitHub handoff is available from the desktop launcher." });
        return;
      }
      await invoke("open_publishing_link", { kind, repository, modId: form.id, name: form.name, version: form.version });
    });
  }

  const prompt = [
    "Prepare a CoD2x Launcher release using the package helper in https://github.com/laspencer91/cod2x-launcher.",
    "Read docs/publishing.md. Use tools/New-ModPackage.ps1 with -ReleaseAssets.",
    "Use a clean build output folder. Confirm the exact mod payload paths with me before packaging.",
    "Package only the custom mss32.dll if this release replaces it, and the intended main/*.iwd files.",
    "Keep commercial game files, original Miles DLL, game configs, and demos out of the package.",
    "",
    "Mod name: " + (form.name || "[display name]"),
    "Mod ID: " + (form.id || "[stable lower-case id]"),
    "Version: " + (form.version || "[version]"),
    "Required official baseline: " + (form.baseVersion || "[official base]"),
    "Authors: " + (authorNames.join(", ") || "[public credits]"),
    "Description: " + (form.description || "[short description]"),
    "Release notes: " + (form.releaseNotes || "[notes]"),
    "GitHub repository: " + (repository || "[owner/repo]"),
    "Files to review: " + (selectedFiles.map(file => file.path).join(", ") || "[exact relative paths]"),
    "",
    "Return mod.zip, mod.json, the SHA-256, and the tag v" + (form.version || "[version]") + " for review.",
    "Use GitHub Releases in the mod's own public repository. Leave submission and publication to me.",
  ].join("\n");

  function copyPrompt() {
    void run("Copying prompt", async () => {
      await navigator.clipboard.writeText(prompt);
      setMessage({ text: "Copied the packaging prompt." });
    });
  }

  function go(next: typeof step) {
    setMessage(null);
    setStep(next);
    requestAnimationFrame(() => document.getElementById("publisher-step-title")?.focus());
  }

  const steps = [{ key: "details", name: "Release details" }, { key: "build", name: "Build files" }, { key: "github", name: "GitHub" }] as const;
  const currentStep = steps.findIndex(item => item.key === step);

  return <>
    <header className="page-heading publisher-heading">
      <div>
        <button className="text-action publisher-back" onClick={onBack}><ArrowLeft size={14} /> Other Mods</button>
        <h1>Publish a mod</h1>
        <p>Package your build and publish it from your own GitHub repo.</p>
      </div>
    </header>
    <ol className="publisher-steps" aria-label="Publishing steps">
      {steps.map((item, index) => <li key={item.key} aria-current={item.key === step ? "step" : undefined}>
        {index < currentStep
          ? <button type="button" className="text-action" disabled={Boolean(busy)} onClick={() => go(item.key)}>{item.name}</button>
          : <span>{item.name}</span>}
      </li>)}
    </ol>
    {step === "details" && <form className="publisher" onSubmit={event => { event.preventDefault(); go("build"); }}>
      <section className="panel publisher-panel">
        <h2 id="publisher-step-title" tabIndex={-1}>Release details</h2>
        <fieldset className="publish-mode">
          <legend>What are you publishing?</legend>
          <label><input type="radio" name="publish-mode" checked={mode === "listing"} onChange={() => setMode("listing")} /><span>List a new mod</span></label>
          <label><input type="radio" name="publish-mode" checked={mode === "update"} onChange={() => setMode("update")} /><span>Publish a new version</span></label>
        </fieldset>
        <p className="publisher-note">{mode === "listing" ? "Publish the first release, then request a listing in the launcher catalog."
          : "Use the same mod ID and GitHub repo. Enter a new version number for the updated build."}</p>
        <div className="publisher-fields">
          {fields.map(({ field, label, placeholder, required, max }) => <label key={field}>
            <span>{label}</span>
            <input value={form[field]} placeholder={placeholder} required={required} maxLength={max}
              pattern={field === "id" || field === "version" ? "[a-z0-9][a-z0-9._\\-]*" : undefined}
              onChange={event => change(field, event.target.value)} />
          </label>)}
          <label className="publisher-wide">
            <span>Official CoD2x baseline</span>
            <select required value={form.baseVersion} aria-describedby="baseline-help" onChange={event => change("baseVersion", event.target.value)}>
              {[...new Set([baseVersion, ...baseVersions, form.baseVersion])].map(version => <option value={version} key={version}>{version}</option>)}
            </select>
            <small id="baseline-help">Choose the official setup your users should start from. For a custom client, it supplies the original audio DLL. Your mod has its own version above.</small>
          </label>
          <label className="publisher-wide"><span>Short description</span><input required maxLength={250} placeholder="What does this build add?" value={form.description} onChange={event => change("description", event.target.value)} /></label>
          <label className="publisher-wide"><span>Release notes</span><textarea rows={3} maxLength={4000} placeholder="What changed in this version?" value={form.releaseNotes} onChange={event => change("releaseNotes", event.target.value)} /></label>
        </div>
        <div className="publisher-section-foot">
          <button type="button" className="text-action" onClick={() => open("new")} disabled={Boolean(busy)}><Github size={14} /> Create a mod repo on GitHub</button>
          <button type="submit" className="button button-primary" disabled={Boolean(busy)}>Continue to build files <ArrowRight size={15} /></button>
        </div>
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
        {!validRepository && <p className="publisher-note">Add your mod repo in Release details to open GitHub.</p>}
      </section>
      {mode === "listing" && <section className="panel publisher-panel">
        <h2>Request a listing</h2>
        <p>Once the release is public, open the listing form in the catalog repo. Submit it on GitHub. Future releases come from the same approved repo.</p>
        <button className="button button-secondary publisher-request" type="button" disabled={!validRepository || Boolean(busy)} onClick={() => open("listing")}><Send size={15} /> Open listing request</button>
      </section>}
    </div>}
    {step !== "github" && <div className="publisher-assistance">
      <aside className="panel publisher-panel publisher-section-head">
        <div><h2>Using an AI coding agent?</h2><p>Copy your release details and selected file paths. Paste them into your agent to prepare the same ZIP and descriptor with the package script.</p></div>
        <button type="button" className="button button-secondary" onClick={copyPrompt} disabled={Boolean(busy)}><ClipboardCopy size={16} /> Copy packaging instructions</button>
      </aside>
    </div>}
    {(busy || message) && <p className={"publisher-feedback" + (message?.error ? " publisher-error" : "")} role={message?.error ? "alert" : "status"}>{busy || message?.text}</p>}
  </>;
}
