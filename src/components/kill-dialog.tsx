import { useEffect, useMemo, useState } from "preact/hooks";
import { api } from "../api";
import { bytes, shortRole } from "../format";
import { endImpact } from "../impact";
import type { ProcessInfo } from "../types";
import { Button, ImpactBadge, ImpactDetails } from "./ui";

export interface KillRequest {
  targets: ProcessInfo[];
  tree: boolean;
  /** Everything that will end, children included. Used for the warning and totals. */
  affected: ProcessInfo[];
  /** Direct children left running, for a single process ended without its tree. */
  childCount: number;
}

export function descendants(root: ProcessInfo, all: ProcessInfo[]): ProcessInfo[] {
  const out: ProcessInfo[] = [];
  const seen = new Set([root.key]);
  const queue = [root.key];
  while (queue.length) {
    const parent = queue.shift()!;
    for (const p of all) {
      if (p.parentKey === parent && !seen.has(p.key)) {
        seen.add(p.key);
        out.push(p);
        queue.push(p.key);
      }
    }
  }
  return out;
}

export function KillDialog(props: {
  request: KillRequest;
  onClose: () => void;
  onDone: (message: string) => void;
}) {
  const { request, onClose, onDone } = props;
  const [busy, setBusy] = useState(false);
  const { affected } = request;
  const total = affected.reduce((sum, p) => sum + p.private, 0);
  const n = affected.length;
  const single = n === 1 ? affected[0] : null;
  // The largest first, since those are what the decision is really about.
  const sorted = useMemo(() => [...affected].sort((a, b) => b.private - a.private), [affected]);
  const blocked = affected.filter((p) => endImpact(p).risk === "blocked").length;

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape" && !busy) onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [busy, onClose]);

  const confirm = async () => {
    setBusy(true);
    let killed = 0;
    const failures: string[] = [];
    for (const target of request.targets) {
      try {
        const outcome = await api.kill(target.key, request.tree);
        killed += outcome.killed.length;
        failures.push(...outcome.failed.map((f) => `${f.name}: ${f.reason}`));
      } catch (e) {
        failures.push(`${target.name}: ${String(e)}`);
      }
    }
    const summary = `Ended ${killed} ${killed === 1 ? "process" : "processes"}.`;
    onDone(failures.length ? `${summary} ${failures.length} could not be ended. ${failures[0]}` : summary);
  };

  return (
    <div class="fixed inset-0 z-50 flex items-center justify-center bg-black/50" role="presentation">
      <div
        role="alertdialog"
        aria-modal="true"
        aria-labelledby="kill-title"
        class="w-[480px] max-h-[80vh] flex flex-col rounded-lg bg-surface border border-line shadow-xl"
      >
        <div class="px-5 pt-4 pb-3">
          <h2 id="kill-title" class="text-[15px] font-semibold m-0">
            {single ? `End ${shortRole(single)}?` : `End ${n} processes?`}
          </h2>
          <div class="text-ink-3 text-xs mt-0.5">
            {single ? `${single.appLabel} · ` : ""}Frees about {bytes(total)} · Stops immediately, with no chance to save
          </div>
        </div>
        {single ? (
          <div class="border-y border-line px-5 py-3">
            <div class="font-semibold mb-1">
              <ImpactBadge impact={endImpact(single)} />
            </div>
            <ImpactDetails impact={endImpact(single, request.childCount)} />
          </div>
        ) : (
          <ul class="flex-1 overflow-y-auto border-y border-line px-5 py-2 m-0 list-none">
            {sorted.slice(0, 200).map((p) => (
              <li key={p.key} class="grid grid-cols-[minmax(0,1fr)_140px_56px] items-center gap-3 py-0.5">
                <span class="truncate" title={`${p.appLabel}: ${p.name}`}>{shortRole(p)}</span>
                <span class="min-w-0 flex text-xs"><ImpactBadge impact={endImpact(p)} /></span>
                <span class="tabular text-ink-2 text-right">{bytes(p.private)}</span>
              </li>
            ))}
          </ul>
        )}
        {blocked > 0 && <div class="px-5 pt-2 text-xs text-ink-3">{blocked} protected {blocked === 1 ? "process" : "processes"} will be skipped.</div>}
        <div class="flex justify-end gap-2 px-5 py-3">
          <Button onClick={onClose} disabled={busy}>Cancel</Button>
          <Button tone="danger" onClick={confirm} disabled={busy}>
            {busy ? "Ending" : `End ${n === 1 ? "process" : `${n} processes`}`}
          </Button>
        </div>
      </div>
    </div>
  );
}
