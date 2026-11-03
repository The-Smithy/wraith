import type { ComponentChildren } from "preact";
import { useEffect, useMemo, useState } from "preact/hooks";
import { api } from "../api";
import { bytes, CATEGORY_LABEL, count, cpu, duration, parentExitMatters, rate, shortRole } from "../format";
import { childCounts, endImpact } from "../impact";
import { chainOf } from "../tree";
import type { Bucket, ProcessDetail, ProcessHistory, ProcessInfo } from "../types";
import { LineChart } from "./chart";
import { descendants } from "./kill-dialog";
import { Button, Chip, ImpactBadge, ImpactDetails, Segmented } from "./ui";

const PRESENCE: Record<ProcessInfo["presence"], string> = {
  window: "Has a window",
  service: "Windows service",
  child: "Runs inside an app",
  background: "Independent",
};

type Range = "fine" | "lifetime";
type Tab = "stats" | "details" | "children";

function Fact(props: { label: string; value: string; title?: string }) {
  return (
    <div class="min-w-0" title={props.title}>
      <div class="text-ink-3 text-xs">{props.label}</div>
      <div class="tabular truncate">{props.value}</div>
    </div>
  );
}

function Row(props: { label: string; children: ComponentChildren }) {
  return (
    <div class="grid grid-cols-[76px_minmax(0,1fr)] gap-2 py-0.5">
      <span class="text-ink-3 text-xs pt-px">{props.label}</span>
      <span class="min-w-0">{props.children}</span>
    </div>
  );
}

function PathRow(props: { label: string; value: string; reveal?: boolean; onCopied: () => void }) {
  if (!props.value) return null;
  return (
    <div class="mb-3">
      <div class="flex items-center justify-between">
        <span class="text-ink-3 text-xs">{props.label}</span>
        <span class="flex gap-2">
          {props.reveal && (
            <button type="button" class="bg-transparent border-0 p-0 text-xs text-accent hover:underline" onClick={() => void api.reveal(props.value).catch(() => undefined)}>
              Show in Explorer
            </button>
          )}
          <button
            type="button"
            class="bg-transparent border-0 p-0 text-xs text-accent hover:underline"
            onClick={() => void navigator.clipboard.writeText(props.value).then(props.onCopied)}
          >
            Copy
          </button>
        </span>
      </div>
      <div class="selectable font-mono text-[11px] break-all text-ink-2 max-h-32 overflow-y-auto mt-0.5">{props.value}</div>
    </div>
  );
}

const points = (buckets: Bucket[], pick: (b: Bucket) => number) => buckets.map((b): [number, number] => [b.t, pick(b)]);

function SmallChart(props: { title: string; children: ComponentChildren }) {
  return (
    <div>
      <div class="text-xs text-ink-2 mb-0.5">{props.title}</div>
      {props.children}
    </div>
  );
}

export function Drawer(props: {
  process: ProcessInfo;
  all: ProcessInfo[];
  byKey: ReadonlyMap<string, ProcessInfo>;
  now: number;
  seq: number;
  onClose: () => void;
  onSelect: (key: string) => void;
  onKill: (tree: boolean) => void;
  onToast: (message: string) => void;
}) {
  const { process: p, all, byKey, now, seq, onClose, onSelect, onKill, onToast } = props;
  const [detail, setDetail] = useState<ProcessDetail | null>(null);
  const [history, setHistory] = useState<ProcessHistory | null>(null);
  const [range, setRange] = useState<Range>("fine");
  const [tab, setTab] = useState<Tab>("stats");

  useEffect(() => {
    setDetail(null);
    let live = true;
    void api.processDetail(p.key).then((d) => live && setDetail(d));
    return () => {
      live = false;
    };
  }, [p.key]);

  useEffect(() => {
    if (tab !== "stats") return;
    let live = true;
    void api.processHistory(p.key).then((h) => live && setHistory(h));
    return () => {
      live = false;
    };
  }, [p.key, seq, tab]);

  const buckets = history ? (range === "fine" ? history.fine : history.lifetime) : [];
  const chain = useMemo(() => chainOf(p, byKey), [p, byKey]);
  const children = useMemo(() => all.filter((c) => c.parentKey === p.key).sort((a, b) => b.private - a.private), [all, p.key]);
  const tree = useMemo(() => descendants(p, all), [p, all]);
  const treeTotal = tree.reduce((sum, c) => sum + c.private, p.private);
  const impact = endImpact(p, childCounts(all).get(p.key) ?? 0);
  const copied = () => onToast("Copied");
  const canEnd = impact.risk !== "blocked";
  const parents = chain.slice(0, -1);

  return (
    <aside
      aria-label="Process details"
      class="absolute inset-y-0 right-0 w-[460px] max-w-full z-30 bg-surface border-l border-line shadow-2xl flex flex-col"
    >
      <header class="px-4 pt-3 pb-2 border-b border-line">
        <div class="flex items-start justify-between gap-2">
          <h2 class="text-[15px] font-semibold truncate selectable m-0">{shortRole(p)}</h2>
          <button type="button" aria-label="Close details" onClick={onClose} class="bg-transparent border-0 text-ink-3 hover:text-ink text-lg leading-none px-1">
            ×
          </button>
        </div>
        <div class="flex flex-wrap gap-1 mt-1">
          <Chip tone="accent">{CATEGORY_LABEL[p.category]}</Chip>
          <Chip>{PRESENCE[p.presence]}</Chip>
          {parentExitMatters(p) && <Chip tone="warn">Orphaned</Chip>}
          {p.limited && <Chip tone="warn">Numbers unavailable</Chip>}
        </div>
      </header>

      <div class="flex-1 overflow-y-auto">
        <section aria-label="What this belongs to" class="px-4 py-2.5 border-b border-line">
          <Row label="App">
            <span class="font-semibold">{p.appLabel}</span>
            {detail?.publisher && <span class="text-ink-3 text-xs"> · {detail.publisher}</span>}
          </Row>
          {p.project && (
            <Row label="Project">
              <span class="truncate block" title={p.project.path}>{p.project.name}</span>
            </Row>
          )}
          {parents.length > 0 && (
            <Row label="Started by">
              <span class="flex flex-wrap items-center gap-x-1">
                {parents.map((c, i) => (
                  <span key={c.key} class="inline-flex items-center gap-1">
                    {i > 0 && <span class="text-ink-3">›</span>}
                    <button type="button" onClick={() => onSelect(c.key)} class="bg-transparent border-0 p-0 text-accent hover:underline text-left">
                      {shortRole(c)}
                    </button>
                  </span>
                ))}
              </span>
            </Row>
          )}
          {parents.length === 0 && parentExitMatters(p) && <Row label="Started by"><span class="text-ink-2">Something that has since exited</span></Row>}
          <Row label="Process">
            <span class="text-ink-2 tabular">{p.name} · PID {p.pid}</span>
          </Row>
        </section>

        <section aria-label="If you end this process" class="px-4 py-2.5 border-b border-line">
          <div class="flex items-center justify-between gap-2 mb-1">
            <h3 class="text-xs text-ink-3 font-normal m-0">If you end this</h3>
            <span class="font-semibold flex min-w-0"><ImpactBadge impact={impact} /></span>
          </div>
          <ImpactDetails impact={impact} />
          {canEnd && (
            <div class="flex gap-2 mt-2.5">
              <Button tone="danger" onClick={() => onKill(false)}>End process</Button>
              {tree.length > 0 && (
                <Button onClick={() => onKill(true)}>
                  End with {tree.length} {tree.length === 1 ? "child" : "children"} ({bytes(treeTotal)})
                </Button>
              )}
            </div>
          )}
        </section>

        <div class="px-4 pt-2.5">
          <Segmented
            label="Process information"
            value={tab}
            onChange={setTab}
            options={[
              { value: "stats", label: "Stats" },
              { value: "details", label: "Details" },
              { value: "children", label: `Children (${children.length})` },
            ]}
          />
        </div>

        {tab === "stats" && (
          <div class="px-4 py-3">
            <div class="grid grid-cols-4 gap-x-3 gap-y-2.5">
              <Fact label="Private" value={p.limited ? "—" : bytes(p.private)} title="Memory committed to this process alone" />
              <Fact label="Working set" value={p.limited ? "—" : bytes(p.workingSet)} title="Physical memory in use, including shared pages" />
              <Fact label="Task Mgr" value={p.limited ? "—" : bytes(p.privateWs)} title="Private working set, the Memory column in Task Manager" />
              <Fact label="CPU" value={`${cpu(p.cpu)}%`} />
              <Fact label="Handles" value={p.limited ? "—" : count(p.handles)} />
              <Fact label="Threads" value={count(p.threads)} />
              <Fact label="GDI / USER" value={p.limited ? "—" : `${p.gdi} / ${p.userObjects}`} />
              <Fact label="Uptime" value={p.startKnown ? duration(now - p.startTime) : "—"} />
              <Fact label="Disk read" value={rate(p.readRate)} />
              <Fact label="Disk write" value={rate(p.writeRate)} />
              <Fact label="Connections" value={count(p.connections)} />
              <Fact label="User" value={p.user || "—"} />
            </div>

            <div class="flex items-center justify-between mt-4 mb-1">
              <h3 class="font-semibold m-0">History</h3>
              <Segmented
                label="Chart range"
                value={range}
                onChange={setRange}
                options={[
                  { value: "fine", label: "Recent" },
                  { value: "lifetime", label: "All" },
                ]}
              />
            </div>
            <LineChart
              height={140}
              byteScale
              format={bytes}
              series={[
                { name: "Private", color: "var(--series-1)", points: points(buckets, (b) => b.p * 1024) },
                { name: "Working set", color: "var(--series-2)", points: points(buckets, (b) => b.w * 1024) },
              ]}
            />
            <div class="grid grid-cols-2 gap-3 mt-2">
              <SmallChart title="Handles">
                <LineChart height={90} minMax={2} format={count} series={[{ name: "Handles", color: "var(--series-1)", points: points(buckets, (b) => b.h) }]} />
              </SmallChart>
              <SmallChart title="Threads">
                <LineChart height={90} minMax={2} format={count} series={[{ name: "Threads", color: "var(--series-1)", points: points(buckets, (b) => b.n) }]} />
              </SmallChart>
              <SmallChart title="CPU %">
                <LineChart height={90} format={(v) => `${v < 10 ? v.toFixed(1) : Math.round(v)}%`} series={[{ name: "CPU", color: "var(--series-1)", points: points(buckets, (b) => b.c) }]} />
              </SmallChart>
              <SmallChart title="GDI objects">
                <LineChart height={90} minMax={2} format={count} series={[{ name: "GDI objects", color: "var(--series-1)", points: points(buckets, (b) => b.g) }]} />
              </SmallChart>
            </div>
          </div>
        )}

        {tab === "details" && (
          <div class="px-4 py-3">
            {p.ports.length > 0 && (
              <div class="mb-3">
                <div class="text-ink-3 text-xs">Listening ports</div>
                <div class="tabular">{p.ports.join(", ")}</div>
              </div>
            )}
            {p.services.length > 0 && (
              <div class="mb-3">
                <div class="text-ink-3 text-xs">Hosts services</div>
                <ul class="m-0 p-0 list-none">
                  {p.services.map((s) => (
                    <li key={s}>{s}</li>
                  ))}
                </ul>
              </div>
            )}
            {p.windowTitle && (
              <div class="mb-3">
                <div class="text-ink-3 text-xs">Window</div>
                <div class="selectable">{p.windowTitle}</div>
              </div>
            )}
            {p.project && <PathRow label="Project" value={p.project.path} reveal onCopied={copied} />}
            <PathRow label="Command line" value={detail?.cmd ?? ""} onCopied={copied} />
            <PathRow label="Working folder" value={detail?.cwd ?? ""} reveal onCopied={copied} />
            <PathRow label="Executable" value={p.exe} reveal onCopied={copied} />
          </div>
        )}

        {tab === "children" && (
          <div class="px-3 py-2">
            {children.length === 0 ? (
              <div class="text-ink-3 px-1 py-2">This process has not started anything.</div>
            ) : (
              children.map((c) => (
                <button
                  key={c.key}
                  type="button"
                  onClick={() => onSelect(c.key)}
                  class="w-full grid grid-cols-[minmax(0,1fr)_130px_56px] items-center gap-2 bg-transparent border-0 px-1 py-1 rounded hover:bg-raised text-left"
                >
                  <span class="truncate">{shortRole(c)}</span>
                  <span class="flex min-w-0 text-xs"><ImpactBadge impact={endImpact(c)} /></span>
                  <span class="tabular text-ink-2 text-right">{bytes(c.private)}</span>
                </button>
              ))
            )}
          </div>
        )}
      </div>
    </aside>
  );
}
