import type { Category, Metric } from "./types";

const KB = 1024;
const MB = KB * 1024;
const GB = MB * 1024;

export function bytes(n: number): string {
  if (n >= GB) return `${(n / GB).toFixed(1)} GB`;
  if (n >= MB) return `${Math.round(n / MB)} MB`;
  if (n >= KB) return `${Math.round(n / KB)} KB`;
  return `${Math.round(n)} B`;
}

export function rate(n: number): string {
  return n < KB ? "0" : `${bytes(n)}/s`;
}

export function duration(secs: number): string {
  const s = Math.max(0, Math.floor(secs));
  if (s < 60) return `${s}s`;
  const m = Math.floor(s / 60);
  if (m < 60) return `${m}m`;
  const h = Math.floor(m / 60);
  if (h < 24) return `${h}h ${m % 60}m`;
  return `${Math.floor(h / 24)}d ${h % 24}h`;
}

export function count(n: number): string {
  return Math.round(n).toLocaleString("en-US");
}

export function percent(part: number, whole: number): number {
  return whole > 0 ? Math.round((part / whole) * 100) : 0;
}

export function cpu(n: number): string {
  if (n < 0.05) return "0";
  return n < 10 ? n.toFixed(1) : Math.round(n).toString();
}

/** Time label for a chart axis. Long spans need the date more than the minutes. */
export function clock(t: number, spanSecs: number): string {
  const d = new Date(t * 1000);
  if (spanSecs > 2 * 86400) {
    return d.toLocaleDateString("en-US", { month: "short", day: "numeric" });
  }
  return d.toLocaleTimeString("en-US", { hour: "numeric", minute: "2-digit" });
}

export function metricIsBytes(metric: Metric): boolean {
  return metric === "memory" || metric === "nonpagedPool";
}

export function metricValue(metric: Metric, v: number): string {
  return metricIsBytes(metric) ? bytes(v) : count(v);
}

export const METRIC_LABEL: Record<Metric, string> = {
  memory: "Memory",
  handles: "Handles",
  threads: "Threads",
  gdi: "GDI objects",
  userObjects: "USER objects",
  nonpagedPool: "Non-paged pool",
};

export const CATEGORY_LABEL: Record<Category, string> = {
  editor: "Editor",
  languageServer: "Language server",
  devServer: "Dev server",
  buildTool: "Build tool",
  testRunner: "Test runner",
  packageManager: "Package manager",
  runtime: "Runtime",
  aiAgent: "AI agent",
  mcpServer: "MCP server",
  database: "Database",
  container: "Container",
  terminal: "Terminal",
  versionControl: "Version control",
  browser: "Browser",
  app: "App",
  service: "Service",
  system: "Windows",
  other: "Unreadable",
};

/** A role without the app name in front, for places that already show the app. */
export function shortRole(p: { role: string; tech?: string | null; appLabel: string }): string {
  for (const prefix of [p.tech, p.appLabel]) {
    if (prefix && p.role.startsWith(`${prefix}: `)) return p.role.slice(prefix.length + 2);
  }
  return p.role;
}

/** Launchers exit all the time. It only means something for tools that should have a parent. */
export function parentExitMatters(p: { parentGone: boolean; category: Category }): boolean {
  return p.parentGone && DEV_CATEGORIES.has(p.category) && p.category !== "editor" && p.category !== "terminal";
}

export const DEV_CATEGORIES: ReadonlySet<Category> = new Set<Category>([
  "editor",
  "languageServer",
  "devServer",
  "buildTool",
  "testRunner",
  "packageManager",
  "runtime",
  "aiAgent",
  "mcpServer",
  "database",
  "container",
  "terminal",
  "versionControl",
]);
