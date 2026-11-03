import { useMemo, useState } from "preact/hooks";
import { Chip, Empty, ImpactBadge, Panel, Segmented, Stat } from "../components/ui";
import { bytes, CATEGORY_LABEL, count, DEV_CATEGORIES, duration, parentExitMatters, shortRole } from "../format";
import { endImpact } from "../impact";
import { chainOf } from "../tree";
import type { Category, PortInfo, ProcessInfo, Snapshot } from "../types";

const DEV_PORT_CATEGORIES = new Set<Category>(["devServer", "runtime", "database", "container", "buildTool", "testRunner", "mcpServer", "aiAgent"]);
const JS_RUNTIME = /^(node|bun|deno)(\.exe)?$/i;

type Tab = "ports" | "projects" | "node" | "roles";

function isJsProcess(p: ProcessInfo): boolean {
  if (JS_RUNTIME.test(p.name)) return true;
  // Editors run language servers in Node mode under their own exe name.
  return p.category === "languageServer" && /^(code|cursor|windsurf)/i.test(p.name);
}

const head = "font-normal px-2 py-2";
const cell = "px-2 py-1";

export function DevTools(props: { snapshot: Snapshot; onSelect: (key: string) => void; byKey: ReadonlyMap<string, ProcessInfo> }) {
  const { snapshot, onSelect, byKey } = props;
  const { processes, ports } = snapshot;
  const [tab, setTab] = useState<Tab>("ports");
  const [allPorts, setAllPorts] = useState(false);

  const devProcesses = useMemo(() => processes.filter((p) => DEV_CATEGORIES.has(p.category)), [processes]);
  const devPorts = useMemo(() => ports.filter((p) => DEV_PORT_CATEGORIES.has(p.category) || p.project !== null), [ports]);
  const projects = useMemo(() => groupByProject(devProcesses), [devProcesses]);
  const tooling = useMemo(() => groupByRole(devProcesses), [devProcesses]);
  const jsProcesses = useMemo(() => processes.filter(isJsProcess).sort((a, b) => b.private - a.private), [processes]);

  const sum = (list: ProcessInfo[]) => list.reduce((total, p) => total + p.private, 0);
  const servers = devProcesses.filter((p) => p.category === "devServer");
  const languageServers = devProcesses.filter((p) => p.category === "languageServer");
  const agents = devProcesses.filter((p) => p.category === "aiAgent");

  return (
    <div class="h-full flex flex-col p-4 gap-3">
      <div class="grid grid-cols-4 gap-3">
        <Stat label="All dev tooling" value={bytes(sum(devProcesses))} note={`${devProcesses.length} processes`} />
        <Stat label="Dev servers" value={String(servers.length)} note={bytes(sum(servers))} />
        <Stat label="Language servers" value={String(languageServers.length)} note={bytes(sum(languageServers))} />
        <Stat label="AI agents" value={String(agents.length)} note={bytes(sum(agents))} />
      </div>

      <div class="flex items-center justify-between gap-3">
        <Segmented
          label="Dev tool view"
          value={tab}
          onChange={setTab}
          options={[
            { value: "ports", label: `Ports (${devPorts.length})` },
            { value: "projects", label: `Projects (${projects.length})` },
            { value: "node", label: `Node and JS (${jsProcesses.length})` },
            { value: "roles", label: `Roles (${tooling.length})` },
          ]}
        />
        {tab === "ports" && (
          <label class="flex items-center gap-1.5 text-xs text-ink-2 cursor-pointer">
            <input type="checkbox" class="accent-[var(--accent)]" checked={allPorts} onChange={(e) => setAllPorts((e.target as HTMLInputElement).checked)} />
            Include non-dev ports ({ports.length})
          </label>
        )}
      </div>

      <Panel class="flex-1 min-h-0 overflow-auto">
        {tab === "ports" &&
          ((allPorts ? ports : devPorts).length === 0 ? (
            <Empty title="No dev servers are listening" />
          ) : (
            <PortTable ports={allPorts ? ports : devPorts} byKey={byKey} onSelect={onSelect} />
          ))}

        {tab === "projects" &&
          (projects.length === 0 ? (
            <Empty title="No projects found">A process links to a project when it runs in a folder with .git, package.json or similar.</Empty>
          ) : (
            <div class="grid grid-cols-3 gap-2 p-3">
              {projects.map((project) => (
                <div key={project.path} class="rounded-md border border-line p-2.5 min-w-0">
                  <div class="flex justify-between gap-2">
                    <span class="font-semibold truncate" title={project.path}>{project.name}</span>
                    <span class="tabular text-ink-2 shrink-0">{bytes(project.total)}</span>
                  </div>
                  <ul class="m-0 p-0 list-none mt-1">
                    {project.items.slice(0, 7).map((p) => (
                      <li key={p.key}>
                        <button type="button" onClick={() => onSelect(p.key)} class="w-full flex justify-between gap-2 bg-transparent border-0 p-0 py-px text-left hover:text-accent">
                          <span class="truncate text-ink-2">{shortRole(p)}</span>
                          <span class="tabular text-ink-3 text-xs shrink-0">{p.ports.length ? `:${p.ports[0]} · ` : ""}{bytes(p.private)}</span>
                        </button>
                      </li>
                    ))}
                    {project.items.length > 7 && <li class="text-ink-3 text-xs">and {project.items.length - 7} more</li>}
                  </ul>
                </div>
              ))}
            </div>
          ))}

        {tab === "node" && (
          <table class="w-full border-collapse min-w-[880px] table-fixed">
            <thead class="sticky top-0 bg-surface z-10">
              <tr class="text-xs text-ink-2 text-left">
                <th class={`${head} pl-4`}>What it is</th>
                <th class={`${head} w-[150px]`}>Project</th>
                <th class={`${head} w-[260px]`}>Started by</th>
                <th class={`${head} w-[150px]`}>If ended</th>
                <th class={`${head} pr-4 w-[80px] text-right`}>Private</th>
              </tr>
            </thead>
            <tbody>
              {jsProcesses.slice(0, 100).map((p) => (
                <tr key={p.key} onClick={() => onSelect(p.key)} class="border-t border-line/40 hover:bg-raised cursor-pointer">
                  <td class={`${cell} pl-4 truncate`}>{shortRole(p)}</td>
                  <td class={`${cell} text-ink-2 truncate`}>{p.project?.name ?? ""}</td>
                  <td class={`${cell} text-ink-2 truncate`}>{parentExitMatters(p) ? <Chip tone="warn">Orphaned</Chip> : chainLabel(p, byKey)}</td>
                  <td class={cell}><span class="flex min-w-0"><ImpactBadge impact={endImpact(p)} /></span></td>
                  <td class={`${cell} pr-4 text-right tabular`}>{bytes(p.private)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        )}

        {tab === "roles" && (
          <table class="w-full border-collapse min-w-[720px] table-fixed">
            <thead class="sticky top-0 bg-surface z-10">
              <tr class="text-xs text-ink-2 text-left">
                <th class={`${head} pl-4`}>Role</th>
                <th class={`${head} w-[160px]`}>Type</th>
                <th class={`${head} w-[70px] text-right`}>Count</th>
                <th class={`${head} w-[90px] text-right`}>Largest</th>
                <th class={`${head} pr-4 w-[90px] text-right`}>Total</th>
              </tr>
            </thead>
            <tbody>
              {tooling.map((row) => (
                <tr key={row.role} onClick={() => onSelect(row.largest.key)} class="border-t border-line/40 hover:bg-raised cursor-pointer">
                  <td class={`${cell} pl-4 truncate`}>{row.role}</td>
                  <td class={`${cell} text-ink-2`}>{CATEGORY_LABEL[row.largest.category]}</td>
                  <td class={`${cell} text-right tabular text-ink-2`}>{count(row.count)}</td>
                  <td class={`${cell} text-right tabular text-ink-2`}>{bytes(row.largest.private)}</td>
                  <td class={`${cell} pr-4 text-right tabular`}>{bytes(row.total)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </Panel>
    </div>
  );
}

function chainLabel(p: ProcessInfo, byKey: ReadonlyMap<string, ProcessInfo>): string {
  const chain = chainOf(p, byKey).slice(0, -1);
  if (chain.length === 0) return p.appLabel;
  const parts = chain.slice(-2).map(shortRole);
  return `${p.appLabel} › ${parts.join(" › ")}`;
}

function PortTable(props: { ports: PortInfo[]; byKey: ReadonlyMap<string, ProcessInfo>; onSelect: (key: string) => void }) {
  return (
    <table class="w-full border-collapse min-w-[960px] table-fixed">
      <thead class="sticky top-0 bg-surface z-10">
        <tr class="text-xs text-ink-2 text-left">
          <th class={`${head} pl-4 w-[72px]`}>Port</th>
          <th class={head}>What it is</th>
          <th class={`${head} w-[150px]`}>Project</th>
          <th class={`${head} w-[150px]`}>App</th>
          <th class={`${head} w-[110px]`}>Reachable from</th>
          <th class={`${head} w-[56px] text-right`}>Clients</th>
          <th class={`${head} w-[150px]`}>If ended</th>
          <th class={`${head} w-[72px] text-right`}>Private</th>
          <th class={`${head} pr-4 w-[72px] text-right`}>Uptime</th>
        </tr>
      </thead>
      <tbody>
        {props.ports.map((p) => {
          const owner = p.key ? props.byKey.get(p.key) : undefined;
          return (
            <tr
              key={`${p.port}-${p.pid}`}
              onClick={() => p.key && props.onSelect(p.key)}
              class="border-t border-line/40 hover:bg-raised cursor-pointer"
            >
              <td class={`${cell} pl-4 tabular font-semibold`}>{p.port}</td>
              <td class={`${cell} truncate`}>{owner ? shortRole(owner) : p.role || p.processName}</td>
              <td class={`${cell} text-ink-2 truncate`}>{p.project?.name ?? ""}</td>
              <td class={`${cell} text-ink-2 truncate`}>{p.appLabel}</td>
              <td class={cell}>{p.exposed ? <Chip tone="warn">Network</Chip> : <span class="text-ink-2">This PC</span>}</td>
              <td class={`${cell} text-right tabular text-ink-2`}>{count(p.connections)}</td>
              <td class={cell}><span class="flex min-w-0">{owner ? <ImpactBadge impact={endImpact(owner)} /> : null}</span></td>
              <td class={`${cell} text-right tabular`}>{bytes(p.private)}</td>
              <td class={`${cell} pr-4 text-right tabular text-ink-2`}>{duration(p.uptimeSecs)}</td>
            </tr>
          );
        })}
      </tbody>
    </table>
  );
}

function groupByProject(list: ProcessInfo[]) {
  const map = new Map<string, { name: string; path: string; total: number; items: ProcessInfo[] }>();
  for (const p of list) {
    if (!p.project) continue;
    const entry = map.get(p.project.path) ?? { name: p.project.name, path: p.project.path, total: 0, items: [] };
    entry.total += p.private;
    entry.items.push(p);
    map.set(p.project.path, entry);
  }
  const groups = [...map.values()];
  for (const g of groups) {
    g.items.sort((a, b) => b.ports.length - a.ports.length || b.private - a.private);
  }
  return groups.sort((a, b) => b.total - a.total);
}

function groupByRole(list: ProcessInfo[]) {
  const map = new Map<string, { role: string; count: number; total: number; largest: ProcessInfo }>();
  for (const p of list) {
    const entry = map.get(p.role);
    if (entry) {
      entry.count += 1;
      entry.total += p.private;
      if (p.private > entry.largest.private) entry.largest = p;
    } else {
      map.set(p.role, { role: p.role, count: 1, total: p.private, largest: p });
    }
  }
  return [...map.values()].sort((a, b) => b.total - a.total);
}
