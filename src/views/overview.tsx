import { useEffect, useState } from "preact/hooks";
import { api } from "../api";
import { LineChart } from "../components/chart";
import { ConfidenceBadge, Empty, Panel, Segmented, SeverityBadge, Stat } from "../components/ui";
import { bytes, count, duration, percent } from "../format";
import type { Snapshot, SystemHistory } from "../types";
import type { ViewId } from "../app";

const KIB = 1024;

export function Overview(props: { snapshot: Snapshot; onNavigate: (view: ViewId, query?: string) => void; onSelect: (key: string) => void }) {
  const { snapshot, onNavigate, onSelect } = props;
  const { system, apps, insights, leaks } = snapshot;
  const [history, setHistory] = useState<SystemHistory | null>(null);
  const [range, setRange] = useState<"fine" | "lifetime">("lifetime");

  useEffect(() => {
    let live = true;
    void api.systemHistory().then((h) => live && setHistory(h));
    return () => {
      live = false;
    };
  }, [snapshot.seq]);

  const series = history ? history[range] : [];
  const suspected = leaks.filter((l) => l.confidence === "high" || l.confidence === "medium" || l.confidence === "low");
  const topApps = apps.slice(0, 12);
  const largest = topApps[0]?.private ?? 1;
  const ramPct = percent(system.usedMemory, system.totalMemory);
  const commitPct = percent(system.commitTotal, system.commitLimit);
  const isWindows = system.platform === "windows";
  const reclaimable = insights.reduce((sum, i) => sum + i.reclaimable, 0);

  return (
    <div class="h-full overflow-y-auto p-4 space-y-3">
      <div class="grid grid-cols-5 gap-3">
        <Stat label="RAM in use" value={`${ramPct}%`} bar={ramPct} note={`${bytes(system.usedMemory)} of ${bytes(system.totalMemory)}`} />
        {system.commitLimit > 0 ? (
          <Stat label="Commit charge" value={`${commitPct}%`} bar={commitPct} note={`${bytes(system.commitTotal)} of ${bytes(system.commitLimit)}`} />
        ) : (
          <Stat label="Committed" value={bytes(system.commitTotal)} note="The kernel enforces no limit" />
        )}
        <Stat label="CPU" value={`${Math.round(system.cpu)}%`} bar={system.cpu} note={`${system.cpuCount} logical cores`} />
        <Stat label="Processes" value={count(system.processCount)} note={`${count(system.threadCount)} threads, ${count(system.handleCount)} handles`} />
        <Stat label="PC uptime" value={duration(system.uptimeSecs)} note={`Monitoring for ${duration(snapshot.t - snapshot.monitorSince)}`} />
      </div>

      <div class="grid grid-cols-[minmax(0,3fr)_minmax(0,2fr)] gap-3">
        <Panel
          title="Memory over time"
          aside={
            <Segmented
              label="Chart range"
              value={range}
              onChange={setRange}
              options={[
                { value: "fine", label: "Recent" },
                { value: "lifetime", label: "All" },
              ]}
            />
          }
        >
          <div class="px-3 pb-3">
            <LineChart
              height={210}
              byteScale
              format={bytes}
              series={[
                { name: "RAM in use", color: "var(--series-1)", points: series.map((b): [number, number] => [b.t, b.used * KIB]) },
                { name: "Commit charge", color: "var(--series-2)", points: series.map((b): [number, number] => [b.t, b.commit * KIB]) },
              ]}
            />
          </div>
        </Panel>

        <Panel title="Needs attention" aside={reclaimable > 0 ? <span class="text-xs text-ink-2 tabular">Up to {bytes(reclaimable)} to free</span> : undefined}>
          <div class="px-2 pb-2 max-h-[248px] overflow-y-auto">
            {suspected.length === 0 && insights.length === 0 && <Empty title="Nothing to flag right now" />}
            {suspected.slice(0, 4).map((l) => (
              <button
                key={l.id}
                type="button"
                onClick={() => (l.key ? onSelect(l.key) : onNavigate("leaks"))}
                class="w-full grid grid-cols-[minmax(0,1fr)_auto] items-center gap-3 text-left bg-transparent border-0 rounded px-2 py-1.5 hover:bg-raised"
              >
                <span class="truncate font-medium">Possible leak: {l.role}</span>
                <ConfidenceBadge confidence={l.confidence} />
              </button>
            ))}
            {insights.slice(0, 8).map((i) => (
              <button
                key={i.id}
                type="button"
                onClick={() => onNavigate("insights")}
                class="w-full grid grid-cols-[minmax(0,1fr)_auto] items-center gap-3 text-left bg-transparent border-0 rounded px-2 py-1.5 hover:bg-raised"
              >
                <span class="truncate">{i.title}</span>
                <SeverityBadge severity={i.severity} />
              </button>
            ))}
          </div>
        </Panel>
      </div>

      <div class="grid grid-cols-[minmax(0,3fr)_minmax(0,2fr)] gap-3">
        <Panel title="Where memory goes" aside={<span class="text-xs text-ink-3">Private memory</span>}>
          <div class="px-2 pb-2">
            {topApps.map((app) => (
              <button
                key={app.label}
                type="button"
                onClick={() => onNavigate("processes", app.label)}
                title={`${app.processCount} processes`}
                class="w-full grid grid-cols-[170px_1fr_64px] items-center gap-3 bg-transparent border-0 rounded px-2 py-1 hover:bg-raised text-left"
              >
                <span class="truncate">{app.label}</span>
                <span class="h-2.5 block">
                  <span class="block h-full rounded-r bg-series-1" style={{ width: `${Math.max(1, (app.private / largest) * 100)}%` }} />
                </span>
                <span class="tabular text-right text-ink-2">{bytes(app.private)}</span>
              </button>
            ))}
          </div>
        </Panel>

        <Panel title="Kernel and cache">
          <div class="px-4 pb-2 grid grid-cols-3 gap-3 text-xs">
            <div>
              <div class="text-ink-3">{isWindows ? "Non-paged pool" : "Unreclaimable"}</div>
              <div class="text-[15px] font-semibold tabular">{bytes(system.kernelNonpaged)}</div>
            </div>
            <div>
              <div class="text-ink-3">{isWindows ? "Paged pool" : "Reclaimable"}</div>
              <div class="text-[15px] font-semibold tabular">{bytes(system.kernelPaged)}</div>
            </div>
            <div>
              <div class="text-ink-3">File cache</div>
              <div class="text-[15px] font-semibold tabular">{bytes(system.systemCache)}</div>
            </div>
          </div>
          <div class="px-3 pb-3">
            <div class="text-xs text-ink-2 px-1 pb-0.5" title="Only drivers and the kernel can grow this, so steady growth points at a driver">{isWindows ? "Non-paged pool" : "Unreclaimable kernel memory"}</div>
            <LineChart
              height={130}
              byteScale
              format={bytes}
              series={[{ name: isWindows ? "Non-paged pool" : "Unreclaimable", color: "var(--series-1)", points: series.map((b): [number, number] => [b.t, b.nonpaged * KIB]) }]}
            />
          </div>
        </Panel>
      </div>
    </div>
  );
}
