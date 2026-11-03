import { useEffect, useMemo, useRef, useState } from "preact/hooks";
import { bytes, cpu, count, duration, parentExitMatters, shortRole } from "../format";
import { childCounts as countChildren, endImpact } from "../impact";
import { buildRows, type Filter, type Row, type SortKey, type ViewMode } from "../tree";
import type { ProcessInfo } from "../types";
import { Chip, Empty, ImpactBadge } from "./ui";

const ROW_HEIGHT = 32;
const OVERSCAN = 10;

interface Column {
  label: string;
  width: string;
  sort?: SortKey;
  right?: boolean;
  title?: string;
  /** Rows are already grouped under their app in the other layout. */
  flatOnly?: boolean;
}

const ALL_COLUMNS: Column[] = [
  { label: "Process", width: "minmax(240px,1fr)", sort: "name" },
  { label: "App", width: "170px", sort: "app", flatOnly: true },
  { label: "Project", width: "130px" },
  { label: "If ended", width: "150px", title: "What happens if you end this process. Select a row for the full explanation." },
  { label: "Private", width: "72px", sort: "private", right: true, title: "Memory committed to this process alone. Leaks show up here." },
  { label: "CPU %", width: "50px", sort: "cpu", right: true },
  { label: "Handles", width: "62px", sort: "handles", right: true },
  { label: "Uptime", width: "62px", sort: "uptime", right: true },
  { label: "Ports", width: "84px" },
];

export function ProcessTable(props: {
  processes: ProcessInfo[];
  now: number;
  mode: ViewMode;
  filter: Filter;
  query: string;
  selected: string | null;
  onSelect: (key: string) => void;
}) {
  const { processes, now, mode, filter, query, selected, onSelect } = props;
  const [sort, setSort] = useState<SortKey>("private");
  const [toggled, setToggled] = useState<ReadonlySet<string>>(new Set());
  const [scrollTop, setScrollTop] = useState(0);
  const [viewport, setViewport] = useState(600);
  const scroller = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const el = scroller.current;
    if (!el) return;
    const observer = new ResizeObserver(([entry]) => setViewport(entry.contentRect.height));
    observer.observe(el);
    return () => observer.disconnect();
  }, []);

  const rows = useMemo(
    () => buildRows(processes, { mode, sort, filter, query, toggled }),
    [processes, mode, sort, filter, query, toggled],
  );
  const childCounts = useMemo(() => countChildren(processes), [processes]);

  const columns = mode === "flat" ? ALL_COLUMNS : ALL_COLUMNS.filter((c) => !c.flatOnly);
  const template = columns.map((c) => c.width).join(" ");

  const first = Math.max(0, Math.floor(scrollTop / ROW_HEIGHT) - OVERSCAN);
  const last = Math.min(rows.length, Math.ceil((scrollTop + viewport) / ROW_HEIGHT) + OVERSCAN);

  const toggle = (id: string) => {
    setToggled((current) => {
      const next = new Set(current);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  };

  return (
    <div class="flex-1 min-h-0 overflow-x-auto bg-surface border border-line rounded-lg flex flex-col">
      <div
        ref={scroller}
        class={`${mode === "flat" ? "min-w-[1070px]" : "min-w-[900px]"} flex-1 min-h-0 overflow-y-auto`}
        onScroll={(e) => setScrollTop((e.target as HTMLElement).scrollTop)}
      >
        <div
          role="row"
          class="sticky top-0 z-10 bg-surface grid items-center gap-x-3 px-3 h-8 border-b border-line text-xs text-ink-2"
          style={{ gridTemplateColumns: template }}
        >
          {columns.map((c) =>
            c.sort ? (
              <button
                key={c.label}
                type="button"
                title={c.title}
                onClick={() => setSort(c.sort!)}
                class={`bg-transparent border-0 p-0 text-xs hover:text-ink truncate ${c.right ? "text-right" : "text-left"} ${
                  sort === c.sort ? "text-ink font-semibold" : "text-ink-2"
                }`}
              >
                {c.label}
                {sort === c.sort ? " ↓" : ""}
              </button>
            ) : (
              <span key={c.label} title={c.title} class={c.right ? "text-right" : ""}>
                {c.label}
              </span>
            ),
          )}
        </div>
        {rows.length === 0 ? (
          <Empty title="No matching processes">Try a different search or filter.</Empty>
        ) : (
          <div style={{ height: rows.length * ROW_HEIGHT, position: "relative" }}>
            {rows.slice(first, last).map((row, i) => (
              <TableRow
                key={row.id}
                row={row}
                now={now}
                top={(first + i) * ROW_HEIGHT}
                template={template}
                showApp={mode === "flat"}
                childCount={row.process ? childCounts.get(row.process.key) ?? 0 : 0}
                selected={row.process?.key === selected}
                onToggle={toggle}
                onSelect={onSelect}
              />
            ))}
          </div>
        )}
      </div>
    </div>
  );
}

function TableRow(props: {
  row: Row;
  now: number;
  top: number;
  template: string;
  showApp: boolean;
  childCount: number;
  selected: boolean;
  onToggle: (id: string) => void;
  onSelect: (key: string) => void;
}) {
  const { row, now, top, template, showApp, childCount, selected, onToggle, onSelect } = props;
  const p = row.process;
  const t = row.totals;
  const cell = "text-right tabular truncate";
  const dim = p?.limited ? "—" : null;
  return (
    <div
      role="row"
      onClick={() => (p ? onSelect(p.key) : onToggle(row.id))}
      class={`absolute inset-x-0 grid items-center gap-x-3 px-3 cursor-pointer border-b border-line/40 ${
        selected ? "bg-accent-wash" : p ? "hover:bg-raised" : "bg-raised/40 hover:bg-raised"
      }`}
      style={{ top, height: ROW_HEIGHT, gridTemplateColumns: template }}
    >
      <div class="flex items-center min-w-0" style={{ paddingLeft: row.depth * 14 }}>
        {row.expandable ? (
          <button
            type="button"
            aria-label={row.expanded ? "Collapse" : "Expand"}
            aria-expanded={row.expanded}
            onClick={(e) => {
              e.stopPropagation();
              onToggle(row.id);
            }}
            class="w-5 h-5 shrink-0 bg-transparent border-0 p-0 text-ink-3 hover:text-ink text-[10px]"
          >
            {row.expanded ? "▼" : "▶"}
          </button>
        ) : (
          <span class="w-5 shrink-0" />
        )}
        {p ? (
          <span class="flex items-center gap-1.5 min-w-0">
            <span class="truncate shrink-[1] min-w-[40%]">{shortRole(p)}</span>
            <span class="text-ink-3 text-xs truncate shrink-[20] min-w-0">{p.name}</span>
            {parentExitMatters(p) && <Chip tone="warn">Orphaned</Chip>}
          </span>
        ) : (
          <span class="flex items-center gap-1.5 min-w-0">
            <span class="font-semibold truncate">{row.label}</span>
            <span class="text-ink-3 text-xs whitespace-nowrap tabular">{t.count}</span>
          </span>
        )}
      </div>
      {showApp && <span class="truncate">{p?.appLabel ?? ""}</span>}
      <span class="text-ink-2 truncate">{p?.project?.name ?? ""}</span>
      <span class="min-w-0 flex">{p ? <ImpactBadge impact={endImpact(p, childCount)} /> : null}</span>
      <span class={p ? cell : `${cell} font-semibold`}>{(p && !p.memoryKnown) || (!p && t.private === 0) ? "—" : bytes(t.private)}</span>
      <span class={`${cell} text-ink-2`}>{cpu(t.cpu)}</span>
      <span class={`${cell} text-ink-2`}>{dim ?? count(t.handles)}</span>
      <span class={`${cell} text-ink-2`}>{t.startTime === Infinity ? "—" : duration(now - t.startTime)}</span>
      <span class="text-ink-2 tabular truncate">{p?.ports.join(", ") ?? ""}</span>
    </div>
  );
}
