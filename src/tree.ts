import { DEV_CATEGORIES } from "./format";
import type { ProcessInfo } from "./types";

export type SortKey = "name" | "app" | "private" | "workingSet" | "cpu" | "handles" | "threads" | "uptime";
export type ViewMode = "apps" | "flat";
export type Filter = "all" | "dev" | "editor" | "browser" | "app" | "service" | "system";

export interface Totals {
  private: number;
  workingSet: number;
  cpu: number;
  handles: number;
  threads: number;
  startTime: number;
  count: number;
}

export interface Row {
  id: string;
  depth: number;
  expandable: boolean;
  expanded: boolean;
  /** Set on process rows, absent on app group rows. */
  process?: ProcessInfo;
  label: string;
  totals: Totals;
}

export function matchesFilter(p: ProcessInfo, filter: Filter): boolean {
  switch (filter) {
    case "all":
      return true;
    case "dev":
      return DEV_CATEGORIES.has(p.category) && p.category !== "editor";
    case "editor":
      return p.category === "editor" || p.appLabel === appOfEditor(p);
    case "browser":
      return p.category === "browser";
    case "app":
      return p.category === "app";
    case "service":
      return p.category === "service" || p.presence === "service";
    case "system":
      return p.category === "system" || p.category === "other";
  }
}

function appOfEditor(p: ProcessInfo): string | null {
  return p.category === "languageServer" ? p.appLabel : null;
}

export function matchesQuery(p: ProcessInfo, query: string): boolean {
  if (!query) return true;
  const q = query.toLowerCase();
  return (
    p.name.toLowerCase().includes(q) ||
    p.role.toLowerCase().includes(q) ||
    p.appLabel.toLowerCase().includes(q) ||
    p.cmdShort.toLowerCase().includes(q) ||
    String(p.pid) === q ||
    p.ports.some((port) => String(port) === q) ||
    (p.project?.name.toLowerCase().includes(q) ?? false) ||
    p.services.some((s) => s.toLowerCase().includes(q)) ||
    (p.windowTitle?.toLowerCase().includes(q) ?? false)
  );
}

function ownTotals(p: ProcessInfo): Totals {
  return {
    private: p.private,
    workingSet: p.workingSet,
    cpu: p.cpu,
    handles: p.handles,
    threads: p.threads,
    // Unknown start times must not read as "started just now".
    startTime: p.startKnown ? p.startTime : Infinity,
    count: 1,
  };
}

function addTotals(a: Totals, b: Totals): Totals {
  return {
    private: a.private + b.private,
    workingSet: a.workingSet + b.workingSet,
    cpu: a.cpu + b.cpu,
    handles: a.handles + b.handles,
    threads: a.threads + b.threads,
    startTime: Math.min(a.startTime, b.startTime),
    count: a.count + b.count,
  };
}

function compare(a: Totals, b: Totals, an: string, bn: string, sort: SortKey): number {
  switch (sort) {
    case "name":
    case "app":
      return an.localeCompare(bn);
    case "uptime":
      return a.startTime - b.startTime;
    default:
      return b[sort] - a[sort] || an.localeCompare(bn);
  }
}

interface Options {
  mode: ViewMode;
  sort: SortKey;
  filter: Filter;
  query: string;
  /** App ids here are open, process keys here are closed, because each has a different default. */
  toggled: ReadonlySet<string>;
}

interface Node {
  process: ProcessInfo;
  children: Node[];
  totals: Totals;
}

export function buildRows(processes: ProcessInfo[], options: Options): Row[] {
  const { mode, sort, filter, query, toggled } = options;
  const visible = processes.filter((p) => matchesFilter(p, filter) && matchesQuery(p, query));

  if (mode === "flat") {
    const rows = visible.map<Row>((p) => ({
      id: p.key,
      depth: 0,
      expandable: false,
      expanded: false,
      process: p,
      label: p.role,
      totals: ownTotals(p),
    }));
    if (sort === "app") {
      rows.sort((a, b) => a.process!.appLabel.localeCompare(b.process!.appLabel) || b.totals.private - a.totals.private);
    } else {
      rows.sort((a, b) => compare(a.totals, b.totals, a.label, b.label, sort));
    }
    return rows;
  }

  // Searching should find things inside collapsed groups, so everything opens.
  const forceOpen = query.length > 0;
  const groups = new Map<string, ProcessInfo[]>();
  for (const p of visible) {
    const list = groups.get(p.appLabel);
    if (list) list.push(p);
    else groups.set(p.appLabel, [p]);
  }

  const built = [...groups.entries()].map(([label, list]) => {
    const roots = buildForest(list);
    const totals = roots.map((r) => r.totals).reduce(addTotals);
    return { label, roots, totals };
  });
  built.sort((a, b) => compare(a.totals, b.totals, a.label, b.label, sort));

  const rows: Row[] = [];
  for (const group of built) {
    const id = `app:${group.label}`;
    const open = forceOpen || toggled.has(id);
    rows.push({
      id,
      depth: 0,
      expandable: true,
      expanded: open,
      label: group.label,
      totals: group.totals,
    });
    if (open) {
      for (const root of sortNodes(group.roots, sort)) {
        pushNode(root, 1, rows, sort, toggled, forceOpen);
      }
    }
  }
  return rows;
}

function buildForest(list: ProcessInfo[]): Node[] {
  const nodes = new Map<string, Node>(
    list.map((p) => [p.key, { process: p, children: [], totals: ownTotals(p) }]),
  );
  const roots: Node[] = [];
  for (const node of nodes.values()) {
    const parent = node.process.parentKey ? nodes.get(node.process.parentKey) : undefined;
    if (parent) parent.children.push(node);
    else roots.push(node);
  }
  for (const root of roots) rollUp(root);
  return roots;
}

function rollUp(node: Node): Totals {
  for (const child of node.children) {
    node.totals = addTotals(node.totals, rollUp(child));
  }
  return node.totals;
}

function sortNodes(nodes: Node[], sort: SortKey): Node[] {
  return [...nodes].sort((a, b) => compare(a.totals, b.totals, a.process.role, b.process.role, sort));
}

function pushNode(
  node: Node,
  depth: number,
  rows: Row[],
  sort: SortKey,
  toggled: ReadonlySet<string>,
  forceOpen: boolean,
) {
  const expandable = node.children.length > 0;
  const open = expandable && (forceOpen || !toggled.has(node.process.key));
  rows.push({
    id: node.process.key,
    depth,
    expandable,
    expanded: open,
    process: node.process,
    label: node.process.role,
    // A closed row stands in for its whole subtree, an open one only for itself.
    totals: open ? ownTotals(node.process) : node.totals,
  });
  if (open) {
    for (const child of sortNodes(node.children, sort)) {
      pushNode(child, depth + 1, rows, sort, toggled, forceOpen);
    }
  }
}

/** The parent chain from the top of the tree down to `p`. */
export function chainOf(p: ProcessInfo, byKey: ReadonlyMap<string, ProcessInfo>): ProcessInfo[] {
  const chain = [p];
  let current = p;
  for (let i = 0; i < 24 && current.parentKey; i++) {
    const parent = byKey.get(current.parentKey);
    if (!parent) break;
    chain.unshift(parent);
    current = parent;
  }
  return chain;
}
