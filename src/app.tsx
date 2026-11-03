import type { ComponentChildren } from "preact";
import { useCallback, useEffect, useMemo, useRef, useState } from "preact/hooks";
import { api } from "./api";
import { Drawer } from "./components/drawer";
import { descendants, KillDialog, type KillRequest } from "./components/kill-dialog";
import { bytes, duration, percent } from "./format";
import { childCounts } from "./impact";
import type { ProcessInfo, Settings, Snapshot } from "./types";
import { Background } from "./views/background";
import { DevTools } from "./views/dev";
import { Exited } from "./views/exited";
import { Insights } from "./views/insights";
import { Leaks } from "./views/leaks";
import { Overview } from "./views/overview";
import { Processes, type ProcessViewState } from "./views/processes";
import { SettingsView, type Theme } from "./views/settings";

export type ViewId = "overview" | "processes" | "dev" | "background" | "leaks" | "insights" | "exited" | "settings";

const NAV: { id: ViewId; label: string; icon: string }[] = [
  { id: "overview", label: "Overview", icon: "M3 13h4v8H3zM10 8h4v13h-4zM17 3h4v18h-4z" },
  { id: "processes", label: "Processes", icon: "M4 5h16M4 12h16M4 19h16" },
  { id: "dev", label: "Dev tools", icon: "M8 7l-5 5 5 5M16 7l5 5-5 5M14 4l-4 16" },
  { id: "background", label: "Background", icon: "M12 3v3M12 18v3M3 12h3M18 12h3M12 9a3 3 0 100 6 3 3 0 000-6z" },
  { id: "leaks", label: "Leaks", icon: "M12 3s7 7.5 7 12a7 7 0 01-14 0c0-4.500 7-12 7-12z" },
  { id: "insights", label: "Optimize", icon: "M9 18h6M10 21h4M12 3a6 6 0 00-4 10.500c.800.800 1 1.500 1 2.500h6c0-1 .200-1.700 1-2.500A6 6 0 0012 3z" },
  { id: "exited", label: "Exited", icon: "M12 7v5l3 2M12 3a9 9 0 100 18 9 9 0 000-18z" },
  { id: "settings", label: "Settings", icon: "M4 7h10M18 7h2M4 17h2M10 17h10M14 5v4M6 15v4" },
];

function NavIcon(props: { d: string }) {
  return (
    <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true" class="shrink-0">
      <path d={props.d} />
    </svg>
  );
}

function accessLabel(platform: string, elevated: boolean): string {
  if (platform === "windows") return elevated ? "Running as administrator" : "Not running as administrator";
  return elevated ? "Running as root" : "Running as your user";
}

function readTheme(): Theme {
  try {
    const saved = localStorage.getItem("wraith-theme");
    if (saved === "light" || saved === "dark") return saved;
  } catch {
    // Storage can be unavailable. The default theme is fine then.
  }
  return "system";
}

function useSnapshot() {
  const [snapshot, setSnapshot] = useState<Snapshot | null>(null);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    let seq = 0;
    let timer = 0;
    let live = true;
    const poll = async () => {
      // A window in the tray has no reader, so skip the transfer.
      if (!document.hidden) {
        try {
          const next = await api.snapshot(seq);
          if (!live) return;
          if (next) {
            seq = next.seq;
            setSnapshot(next);
          }
          setError(null);
        } catch (e) {
          if (live) setError(String(e));
        }
      }
      if (live) timer = window.setTimeout(poll, 700);
    };
    void poll();
    return () => {
      live = false;
      window.clearTimeout(timer);
    };
  }, []);
  return { snapshot, error };
}

export function App() {
  const { snapshot, error } = useSnapshot();
  const [view, setView] = useState<ViewId>("overview");
  const [selected, setSelected] = useState<string | null>(null);
  const [settings, setSettings] = useState<Settings | null>(null);
  const [theme, setTheme] = useState<Theme>(readTheme);
  const [kill, setKill] = useState<KillRequest | null>(null);
  const [toast, setToast] = useState<string | null>(null);
  const [processState, setProcessState] = useState<ProcessViewState>({ query: "", filter: "all", mode: "apps" });
  const toastTimer = useRef(0);
  const lastProcess = useRef<ProcessInfo | null>(null);

  useEffect(() => {
    void api.settings().then(setSettings);
  }, []);

  useEffect(() => {
    if (theme === "system") document.documentElement.removeAttribute("data-theme");
    else document.documentElement.setAttribute("data-theme", theme);
    try {
      localStorage.setItem("wraith-theme", theme);
    } catch {
      // The choice just will not persist.
    }
  }, [theme]);

  const showToast = useCallback((message: string) => {
    setToast(message);
    window.clearTimeout(toastTimer.current);
    toastTimer.current = window.setTimeout(() => setToast(null), 5000);
  }, []);

  const saveSettings = useCallback(
    (next: Settings) => {
      setSettings(next);
      api.saveSettings(next).then(setSettings, (e) => showToast(`Could not save settings: ${String(e)}`));
    },
    [showToast],
  );

  const byKey = useMemo(() => new Map(snapshot?.processes.map((p) => [p.key, p]) ?? []), [snapshot]);

  const selectedProcess = selected ? byKey.get(selected) ?? null : null;
  useEffect(() => {
    if (selectedProcess) lastProcess.current = selectedProcess;
    else if (selected && lastProcess.current?.key === selected) {
      showToast(`${lastProcess.current.name} has exited.`);
      setSelected(null);
    }
  }, [selected, selectedProcess, showToast]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape" && !kill) setSelected(null);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [kill]);

  const navigate = useCallback((next: ViewId, query?: string) => {
    setView(next);
    if (query !== undefined) setProcessState((current) => ({ ...current, query, filter: "all" }));
  }, []);

  const requestKill = useCallback(
    (targets: ProcessInfo[], tree: boolean) => {
      const all = snapshot?.processes ?? [];
      const seen = new Map<string, ProcessInfo>();
      for (const t of targets) {
        seen.set(t.key, t);
        if (tree) for (const d of descendants(t, all)) seen.set(d.key, d);
      }
      const childCount = !tree && targets.length === 1 ? childCounts(all).get(targets[0].key) ?? 0 : 0;
      setKill({ targets, tree, affected: [...seen.values()], childCount });
    },
    [snapshot],
  );

  const ignore = useCallback(
    (name: string) => {
      if (settings && !settings.ignored.includes(name)) {
        saveSettings({ ...settings, ignored: [...settings.ignored, name] });
        showToast(`Leak detection will skip ${name}. It leaves the list at the next check.`);
      }
    },
    [settings, saveSettings, showToast],
  );

  if (!snapshot || !settings) {
    return (
      <div class="h-full flex items-center justify-center text-ink-2">
        {error ? `Could not reach the monitor: ${error}` : "Taking the first sample"}
      </div>
    );
  }

  const suspected = snapshot.leaks.filter((l) => l.confidence === "high" || l.confidence === "medium" || l.confidence === "low").length;
  const urgent = snapshot.insights.filter((i) => i.severity !== "info").length;
  const badges: Partial<Record<ViewId, number>> = { leaks: suspected, insights: urgent };
  const title = NAV.find((n) => n.id === view)?.label ?? "";
  const ram = percent(snapshot.system.usedMemory, snapshot.system.totalMemory);

  let content: ComponentChildren;
  switch (view) {
    case "overview":
      content = <Overview snapshot={snapshot} onNavigate={navigate} onSelect={setSelected} />;
      break;
    case "processes":
      content = <Processes snapshot={snapshot} state={processState} onState={setProcessState} selected={selected} onSelect={setSelected} />;
      break;
    case "dev":
      content = <DevTools snapshot={snapshot} onSelect={setSelected} byKey={byKey} />;
      break;
    case "background":
      content = <Background snapshot={snapshot} onSelect={setSelected} />;
      break;
    case "leaks":
      content = <Leaks snapshot={snapshot} settings={settings} onSelect={setSelected} onIgnore={ignore} />;
      break;
    case "insights":
      content = <Insights snapshot={snapshot} byKey={byKey} onSelect={setSelected} onKillMany={(targets) => requestKill(targets, false)} />;
      break;
    case "exited":
      content = <Exited snapshot={snapshot} />;
      break;
    case "settings":
      content = <SettingsView settings={settings} onChange={saveSettings} theme={theme} onTheme={setTheme} />;
      break;
  }

  return (
    <div class="h-full flex">
      <nav aria-label="Main" class="w-44 shrink-0 flex flex-col bg-surface border-r border-line">
        <div class="flex items-center gap-2 px-3.5 h-12">
          <svg width="20" height="20" viewBox="0 0 1024 1024" aria-hidden="true">
            <path d="M176 584h156l76-250 124 412 88-292 56 130h172" fill="none" stroke="var(--accent)" stroke-width="88" stroke-linecap="round" stroke-linejoin="round" />
          </svg>
          <span class="text-[15px] font-semibold tracking-tight">Wraith</span>
        </div>
        <div class="flex-1 px-2 space-y-0.5">
          {NAV.map((item) => {
            const badge = badges[item.id];
            return (
              <button
                key={item.id}
                type="button"
                aria-current={view === item.id ? "page" : undefined}
                onClick={() => setView(item.id)}
                class={`w-full flex items-center gap-2.5 rounded-md border-0 px-2.5 py-1.5 text-left ${
                  view === item.id ? "bg-accent-wash text-ink font-semibold" : "bg-transparent text-ink-2 hover:bg-raised hover:text-ink"
                }`}
              >
                <NavIcon d={item.icon} />
                <span class="flex-1 truncate">{item.label}</span>
                {badge ? <span class="rounded-full bg-critical text-white text-[10px] font-semibold min-w-4 h-4 px-1 inline-flex items-center justify-center">{badge}</span> : null}
              </button>
            );
          })}
        </div>
        <div class="px-3.5 py-3 text-[11px] text-ink-3 leading-relaxed tabular">
          <div>
            Wraith: {bytes(snapshot.selfStats.private)}, {snapshot.selfStats.cpu.toFixed(1)}% CPU
          </div>
          <div>Sample took {snapshot.selfStats.sampleMs} ms</div>
          <div>{accessLabel(snapshot.system.platform, snapshot.system.elevated)}</div>
          {snapshot.system.limitedProcesses > 0 && <div>{snapshot.system.limitedProcesses} processes unreadable</div>}
        </div>
      </nav>

      <div class="flex-1 min-w-0 flex flex-col relative">
        <header class="h-12 shrink-0 flex items-center justify-between px-4 border-b border-line bg-surface">
          <h1 class="text-[15px] font-semibold m-0">{title}</h1>
          <div class="text-xs text-ink-2 tabular">
            RAM {ram}% · {bytes(snapshot.system.availableMemory)} free · up {duration(snapshot.system.uptimeSecs)}
            {error ? <span class="text-critical-ink ml-2">Lost contact with the monitor</span> : null}
          </div>
        </header>
        <main class="flex-1 min-h-0 relative">
          {content}
          {selectedProcess && (
            <Drawer
              key={selectedProcess.key}
              process={selectedProcess}
              all={snapshot.processes}
              byKey={byKey}
              now={snapshot.t}
              seq={snapshot.seq}
              platform={snapshot.system.platform}
              onClose={() => setSelected(null)}
              onSelect={setSelected}
              onKill={(tree) => requestKill([selectedProcess], tree)}
              onToast={showToast}
            />
          )}
        </main>
      </div>

      {kill && (
        <KillDialog
          request={kill}
          platform={snapshot.system.platform}
          onClose={() => setKill(null)}
          onDone={(message) => {
            setKill(null);
            showToast(message);
          }}
        />
      )}
      {toast && (
        <div role="status" class="fixed bottom-4 left-1/2 -translate-x-1/2 z-50 rounded-md border border-line bg-raised px-3 py-2 shadow-lg max-w-[560px]">
          {toast}
        </div>
      )}
    </div>
  );
}
