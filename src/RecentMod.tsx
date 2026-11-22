import { useEffect, useState } from "react";
import { History } from "lucide-react";
import type { HostedRelease, Package, Status } from "./api";

type Recent = { id: string; version: string; name: string; source: "catalog" | "local" | "unknown" };
const storageKey = (path: string) => "recent-mod:" + path.toLowerCase();
function saved(path: string): Recent | null {
  try {
    const value = JSON.parse(localStorage.getItem(storageKey(path)) ?? "null");
    return value && typeof value.id === "string" && typeof value.name === "string" && typeof value.version === "string" &&
      ["catalog", "local", "unknown"].includes(value.source) ? value : null;
  } catch { return null; }
}

export function rememberMod(path: string, mod: Recent) {
  try { localStorage.setItem(storageKey(path), JSON.stringify(mod)); } catch { /* Storage can be unavailable. */ }
}

export function useRecentMod(status: Status | null, hosted: HostedRelease[], cached: Package[]) {
  const [recent, setRecent] = useState<Recent | null>(null);
  useEffect(() => {
    if (!status) { setRecent(null); return; }
    const last = saved(status.path);
    if ((status.kind !== "addon" && status.kind !== "runtime") || !status.packageId || !status.version) { setRecent(last); return; }
    const matches = (p: Package) => p.id === status.packageId && p.version === status.version &&
      p.files.every(file => status.files.some(actual => actual.path === file.path && actual.sha256 === file.sha256));
    const origin = last?.id === status.packageId && last.version === status.version && last.source !== "unknown" ? last.source :
      hosted.some(r => matches(r.manifest)) ? "catalog" : "unknown";
    const active = { id: status.packageId, version: status.version, name: status.name, source: origin } as Recent;
    rememberMod(status.path, active); setRecent(active);
  }, [status, hosted]);
  const last = cached.find(p => p.id === recent?.id && p.version === recent.version);
  const local = last && cached.reduce((best, p) =>
    p.id === last.id && olderNumericVersion(best.version, p.version) ? p : best, last);
  return { recent, local };
}

export function latestMod(releases: HostedRelease[], id: string) {
  return releases.filter(r => r.manifest.id === id)
    .sort((a, b) => (b.publishedAt ?? "").localeCompare(a.publishedAt ?? ""))[0]?.manifest;
}

function olderNumericVersion(candidate: string, used: string) {
  if (!/^\d+(\.\d+)*$/.test(candidate) || !/^\d+(\.\d+)*$/.test(used)) return false;
  const a = candidate.split(".").map(BigInt), b = used.split(".").map(BigInt);
  for (let i = 0; i < Math.max(a.length, b.length); i++) {
    const delta = (a[i] ?? 0n) - (b[i] ?? 0n);
    if (delta !== 0n) return delta < 0n;
  }
  return false;
}

export function proposedMod(releases: HostedRelease[], recent: Recent) {
  const latest = latestMod(releases, recent.id);
  if (latest && !olderNumericVersion(latest.version, recent.version)) return latest;
  return releases.find(r => r.manifest.id === recent.id && r.manifest.version === recent.version)?.manifest;
}

type Props = {
  recent: Recent; local?: Package; releases: HostedRelease[]; available: boolean; loading: boolean;
  active: boolean; disabled: boolean; onCatalog: () => void; onLocal: (mod: Package) => void;
};

export function RecentMod({ recent, local, releases, available, loading, active, disabled, onCatalog, onLocal }: Props) {
  const target = recent.source === "local" ? undefined : proposedMod(releases, recent);
  const localOnly = recent.source === "local" || (recent.source === "unknown" && available && !target);
  const newer = target && target.version !== recent.version;
  const localUpdate = localOnly && local && local.version !== recent.version;
  if (active && !newer && !localUpdate && (available || localOnly)) return null;
  const label = localOnly ? (localUpdate ? "Newer local build" : "Local build") : loading && !available ? "Checking releases…" :
    !available ? "Update check unavailable" : !target ? "Release unavailable" : newer ? "New release" : "Latest release";
  return <div className="recent-mod">
    <History size={16} aria-hidden="true" /><div className="recent-info">
      <span>{active ? "Active mod" : "Last used mod"}</span>
      <strong>{recent.name} <small>v{recent.version}</small></strong>
      <span>{label}</span>
    </div>
    {localOnly && local && (!active || localUpdate) ? <button className="button button-secondary button-small" disabled={disabled} onClick={() => onLocal(local)}>Set up v{local.version}</button> :
      !localOnly && <button className="button button-secondary button-small" disabled={disabled || loading} onClick={onCatalog}>
        {target && available ? (newer ? "Update to v" : "Set up v") + target.version : "Check releases"}</button>}
  </div>;
}
