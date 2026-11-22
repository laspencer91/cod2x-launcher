import { useEffect, useRef, useState } from "react";
import { Channel, invoke } from "@tauri-apps/api/core";
import { Github, RefreshCw } from "lucide-react";
import { HelpTip } from "./HelpTip";
import { native } from "./api";

type Account = { login: string; repositories: { name: string; hasMod: boolean }[] };
export function repoName(value: string) {
  const slug = value.trim().replace(/^https:\/\/github\.com\//i, "").replace(/\/$/, "").replace(/\.git$/i, "");
  return slug.split("/").length === 2 && slug.split("/").every(s => /^[a-z\d_.-]{1,100}$/i.test(s) && s !== "." && s !== "..") ? slug : "";
}

type Props = {
  update: boolean; repository: string; known: Map<string, string>;
  onSelect: (slug: string) => void; onLink: (kind: "new" | "device") => void;
};

export function RepositoryPicker({ update, repository, known, onSelect, onLink }: Props) {
  const [account, setAccount] = useState<Account | null>(null);
  const [loading, setLoading] = useState(native);
  const [connecting, setConnecting] = useState(false);
  const [code, setCode] = useState("");
  const [error, setError] = useState("");
  const [manual, setManual] = useState(false);
  const [link, setLink] = useState("");
  const [revision, setRevision] = useState(0);
  const mounted = useRef(true);
  useEffect(() => {
    mounted.current = true;
    return () => { mounted.current = false; };
  }, []);
  useEffect(() => {
    if (!native) return;
    let current = true;
    setLoading(true);
    setError("");
    invoke<Account | null>("publisher_account").then(value => { if (current) setAccount(value); })
      .catch(e => { if (current) { setError(String(e)); setAccount(null); } })
      .finally(() => { if (current) setLoading(false); });
    return () => { current = false; };
  }, [revision]);

  async function connect() {
    if (!native) { setError("Open the desktop launcher to connect your GitHub account."); return; }
    setConnecting(true);
    setError("");
    const progress = new Channel<string>();
    progress.onmessage = value => { if (mounted.current) setCode(value); };
    try {
      await invoke("connect_publisher", { progress });
      if (mounted.current) setRevision(n => n + 1);
    } catch (e) { if (mounted.current) setError(String(e)); }
    finally { if (mounted.current) { setCode(""); setConnecting(false); } }
  }

  const options = (account?.repositories ?? []).filter(repo => !update || repo.hasMod || known.has(repo.name.toLowerCase()));
  const label = update ? "Published mod" : "Release repository";
  const valueInList = options.some(r => r.name === repository);
  const slug = repoName(link);
  return <section className="repo-picker" aria-label="Publishing repository">
    <div className="repo-head">
      <strong><Github size={16} />{account ? "@" + account.login : "GitHub account"} <HelpTip topic="the connected GitHub account">The launcher uses your active GitHub CLI sign-in to find public repositories where you can create releases. Players can install mods without signing in.</HelpTip></strong>
      {account || error ? <button type="button" className="text-action" disabled={loading || connecting} onClick={() => setRevision(n => n + 1)}><RefreshCw size={13} /> Refresh</button> : null}
    </div>
    {loading ? <p role="status">Finding your repositories…</p> : !account && <div className="repo-connect">
      <p>{update ? "Connect to see mods you can publish updates to." : "Connect to choose a public repository you can publish to."}</p>
      {!connecting && <button type="button" className="button button-secondary" onClick={connect}><Github size={15} /> Connect GitHub</button>}
    </div>}
    {connecting && <div role="status">{code ? <><p>Enter this one-time code on GitHub <strong>{code}</strong></p>
      <button type="button" className="text-action" onClick={() => onLink("device")}>Continue on GitHub ↗</button></> : <p>Starting GitHub sign-in…</p>}</div>}
    {account && <div className="publisher-fields">
      {options.length || repository ? <div className="publisher-wide repo-field"><div className="field-label"><label htmlFor="publish-repo">{label}</label><HelpTip topic={update ? "a published mod" : "a release repository"}>{update ? "Choose a mod that already has a public launcher release. Its latest package supplies the mod details for your update." : "This public GitHub repository hosts your mod’s releases. Choose one you own or help maintain. Players download the version you publish here."}</HelpTip></div>
        <select id="publish-repo" value={repository} onChange={e => onSelect(e.target.value)}>
          <option value="">{update ? "Choose a published mod…" : "Choose a public repository…"}</option>
          {repository && !valueInList && <option value={repository}>{repository}</option>}
          {options.map(r => <option key={r.name} value={r.name}>{update && known.get(r.name.toLowerCase()) ? known.get(r.name.toLowerCase()) + " · " + r.name : r.name}</option>)}
        </select></div> : <p className="publisher-wide">{update ? "No published mods found. Look up a repository below, or list your first mod." : "No writable public repositories found."}</p>}
    </div>}
    {!account && repository && <p className="chosen-repo">{repository}</p>}
    {error && <p role="alert" className="inline-error">{error}</p>}
    <div className="repo-links">
      <button type="button" className="text-action" aria-expanded={manual} onClick={() => { setManual(v => !v); setLink(repository); }}>{manual ? "Close link entry" : "Use a repository link"}</button>
      {!update && <button type="button" className="text-action" onClick={() => onLink("new")}>Create a repository ↗</button>}
    </div>
    {manual && <div className="repo-manual">
      <label>GitHub repository link<input value={link} placeholder="https://github.com/owner/mod" onChange={e => setLink(e.target.value)}
        onKeyDown={e => { if (e.key === "Enter") { e.preventDefault(); if (slug) { onSelect(slug); setManual(false); } } }} /></label>
      <button type="button" className="button button-secondary" disabled={!slug} onClick={() => { onSelect(slug); setManual(false); }}>{update ? "Find release" : "Use repository"}</button>
    </div>}
  </section>;
}
