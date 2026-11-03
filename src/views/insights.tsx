import { useState } from "preact/hooks";
import { Button, Empty, ImpactBadge, Panel, Stat } from "../components/ui";
import { bytes, shortRole } from "../format";
import { endImpact } from "../impact";
import type { Insight, ProcessInfo, Severity, Snapshot } from "../types";

/** Advice where ending the listed processes is itself the recommended fix. */
function offersEnd(insight: Insight): boolean {
  return insight.id === "orphans" || insight.id === "idle-servers";
}

const GROUPS: { severity: Severity; title: string }[] = [
  { severity: "critical", title: "Critical" },
  { severity: "warning", title: "Worth a look" },
  { severity: "info", title: "Tips" },
];

function InsightRow(props: {
  insight: Insight;
  byKey: ReadonlyMap<string, ProcessInfo>;
  open: boolean;
  onToggle: () => void;
  onSelect: (key: string) => void;
  onKillMany: (targets: ProcessInfo[]) => void;
}) {
  const { insight, byKey, open, onToggle, onSelect, onKillMany } = props;
  const processes = insight.keys
    .map((k) => byKey.get(k))
    .filter((p): p is ProcessInfo => p !== undefined)
    .sort((a, b) => b.private - a.private);
  return (
    <div class="border-t border-line/50">
      <button
        type="button"
        aria-expanded={open}
        onClick={onToggle}
        class="w-full grid grid-cols-[16px_minmax(0,1fr)_auto] items-center gap-2 bg-transparent border-0 px-4 py-2 text-left hover:bg-raised"
      >
        <span class="text-ink-3 text-[10px]" aria-hidden="true">{open ? "▼" : "▶"}</span>
        <span class="truncate font-medium">{insight.title}</span>
        <span class="tabular text-ink-2 whitespace-nowrap">{insight.reclaimable > 0 ? `Up to ${bytes(insight.reclaimable)}` : ""}</span>
      </button>
      {open && (
        <div class="px-4 pb-3 pl-10">
          <div class="text-ink-2">{insight.detail}</div>
          <div class="flex items-center justify-between gap-3 mt-1">
            <div><span class="text-ink-3 text-xs">Fix: </span>{insight.action}</div>
            {offersEnd(insight) && processes.length > 0 && (
              <Button tone="danger" onClick={() => onKillMany(processes)}>Review and end</Button>
            )}
          </div>
          {processes.length > 0 && (
            <div class="mt-2 rounded-md border border-line/70">
              {processes.slice(0, 10).map((p) => (
                <button
                  key={p.key}
                  type="button"
                  onClick={() => onSelect(p.key)}
                  class="w-full grid grid-cols-[minmax(0,1fr)_140px_130px_60px] items-center gap-3 bg-transparent border-0 px-2 py-1 text-left hover:bg-raised"
                >
                  <span class="truncate">{shortRole(p)}</span>
                  <span class="truncate text-ink-2">{p.project?.name ?? (shortRole(p) === p.appLabel ? "" : p.appLabel)}</span>
                  <span class="flex min-w-0 text-xs"><ImpactBadge impact={endImpact(p)} /></span>
                  <span class="tabular text-ink-2 text-right">{bytes(p.private)}</span>
                </button>
              ))}
              {processes.length > 10 && <div class="text-ink-3 text-xs px-2 py-1">and {processes.length - 10} more</div>}
            </div>
          )}
        </div>
      )}
    </div>
  );
}

export function Insights(props: {
  snapshot: Snapshot;
  byKey: ReadonlyMap<string, ProcessInfo>;
  onSelect: (key: string) => void;
  onKillMany: (targets: ProcessInfo[]) => void;
}) {
  const { snapshot, byKey, onSelect, onKillMany } = props;
  const { insights } = snapshot;
  const [open, setOpen] = useState<ReadonlySet<string>>(new Set());
  const reclaimable = insights.reduce((sum, i) => sum + i.reclaimable, 0);
  const warnings = insights.filter((i) => i.severity !== "info").length;
  const toggle = (id: string) =>
    setOpen((current) => {
      const next = new Set(current);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });

  return (
    <div class="h-full overflow-y-auto p-4 space-y-3">
      <div class="grid grid-cols-3 gap-3">
        <Stat label="Could be freed" value={bytes(reclaimable)} note="Estimate, some overlap" />
        <Stat label="Worth a look" value={String(warnings)} />
        <Stat label="Tips" value={String(insights.length - warnings)} />
      </div>
      {insights.length === 0 && (
        <Panel>
          <Empty title="Nothing to optimize right now" />
        </Panel>
      )}
      {GROUPS.map((group) => {
        const items = insights.filter((i) => i.severity === group.severity);
        if (items.length === 0) return null;
        return (
          <Panel key={group.severity} title={`${group.title} (${items.length})`}>
            {items.map((insight) => (
              <InsightRow
                key={insight.id}
                insight={insight}
                byKey={byKey}
                open={open.has(insight.id)}
                onToggle={() => toggle(insight.id)}
                onSelect={onSelect}
                onKillMany={onKillMany}
              />
            ))}
          </Panel>
        );
      })}
    </div>
  );
}
