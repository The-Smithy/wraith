import { useMemo, useState } from "preact/hooks";
import { Empty, ImpactBadge, Panel, Segmented } from "../components/ui";
import { bytes, cpu, duration, shortRole } from "../format";
import { childCounts as countChildren, endImpact } from "../impact";
import type { ProcessInfo, Snapshot } from "../types";

type Tab = "independent" | "services" | "all";

function isIndependent(p: ProcessInfo) {
  return (p.presence === "background" || p.presence === "child") && p.category !== "system" && p.category !== "browser" && p.category !== "editor" && p.category !== "languageServer";
}

export function Background(props: { snapshot: Snapshot; onSelect: (key: string) => void }) {
  const { snapshot, onSelect } = props;
  const [tab, setTab] = useState<Tab>("independent");

  const groups = useMemo(() => {
    const windowApps = new Set(snapshot.processes.filter((p) => p.presence === "window").map((p) => p.appKey));
    const bySize = (a: ProcessInfo, b: ProcessInfo) => b.private - a.private;
    return {
      independent: snapshot.processes.filter((p) => isIndependent(p) && !windowApps.has(p.appKey)).sort(bySize),
      services: snapshot.processes.filter((p) => p.services.length > 0).sort(bySize),
      all: snapshot.processes.filter((p) => p.presence !== "window").sort(bySize),
    };
  }, [snapshot.processes]);

  const childCounts = useMemo(() => countChildren(snapshot.processes), [snapshot.processes]);

  const rows = groups[tab];
  const total = rows.reduce((sum, p) => sum + p.private, 0);

  return (
    <div class="h-full flex flex-col p-4 gap-3">
      <div class="flex flex-wrap items-center gap-3">
        <Segmented
          label="Background process type"
          value={tab}
          onChange={setTab}
          options={[
            { value: "independent", label: `Unattended (${groups.independent.length})` },
            { value: "services", label: `Services (${groups.services.length})` },
            { value: "all", label: `All windowless (${groups.all.length})` },
          ]}
        />
        {tab === "independent" && <span class="text-ink-3 text-xs">Running with no window and no open app above them</span>}
        <span class="ml-auto text-ink-2 tabular whitespace-nowrap">{bytes(total)} total</span>
      </div>
      <Panel class="flex-1 min-h-0 overflow-auto">
        {rows.length === 0 ? (
          <Empty title="Nothing here" />
        ) : (
          <table class="w-full border-collapse min-w-[900px] table-fixed">
            <thead class="sticky top-0 bg-surface z-10">
              <tr class="text-xs text-ink-2 text-left">
                <th class="font-normal px-4 py-2">Process</th>
                <th class="font-normal px-2 py-2 w-[260px]">{tab === "services" ? "Services" : "App"}</th>
                <th class="font-normal px-2 py-2 w-[160px]">If ended</th>
                <th class="font-normal px-2 py-2 w-[80px] text-right">Private</th>
                <th class="font-normal px-2 py-2 w-[56px] text-right">CPU %</th>
                <th class="font-normal px-4 py-2 w-[88px] text-right">Uptime</th>
              </tr>
            </thead>
            <tbody>
              {rows.slice(0, 300).map((p) => (
                <tr key={p.key} onClick={() => onSelect(p.key)} class="border-t border-line/40 hover:bg-raised cursor-pointer">
                  <td class="px-4 py-1 truncate">
                    {shortRole(p)} <span class="text-ink-3 text-xs">{p.name}</span>
                  </td>
                  <td class="px-2 py-1 text-ink-2 truncate" title={tab === "services" ? p.services.join(", ") : p.appLabel}>
                    {tab === "services" ? p.services.join(", ") : p.appLabel}
                  </td>
                  <td class="px-2 py-1"><span class="flex min-w-0"><ImpactBadge impact={endImpact(p, childCounts.get(p.key) ?? 0)} /></span></td>
                  <td class="px-2 py-1 text-right tabular">{p.limited ? "—" : bytes(p.private)}</td>
                  <td class="px-2 py-1 text-right tabular text-ink-2">{cpu(p.cpu)}</td>
                  <td class="px-4 py-1 text-right tabular text-ink-2">{p.startKnown ? duration(snapshot.t - p.startTime) : "—"}</td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </Panel>
    </div>
  );
}
