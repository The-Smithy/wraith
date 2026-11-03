// Mirrors src-tauri/src/model.rs. Everything crosses IPC as camelCase JSON.

export type Category =
  | "editor"
  | "languageServer"
  | "devServer"
  | "buildTool"
  | "testRunner"
  | "packageManager"
  | "runtime"
  | "aiAgent"
  | "mcpServer"
  | "database"
  | "container"
  | "terminal"
  | "versionControl"
  | "browser"
  | "app"
  | "service"
  | "system"
  | "other";

export type Presence = "window" | "service" | "child" | "background" | "unknown";

export interface Project {
  name: string;
  path: string;
}

export interface ProcessInfo {
  key: string;
  pid: number;
  parentKey: string | null;
  parentGone: boolean;
  name: string;
  exe: string;
  cmdShort: string;
  startTime: number;
  startKnown: boolean;
  user: string;
  category: Category;
  protected: boolean;
  role: string;
  tech: string | null;
  appLabel: string;
  appKey: string;
  project: Project | null;
  services: string[];
  windowTitle: string | null;
  presence: Presence;
  limited: boolean;
  memoryKnown: boolean;
  private: number;
  workingSet: number;
  privateWs: number;
  cpu: number;
  readRate: number;
  writeRate: number;
  handles: number;
  threads: number;
  gdi: number;
  userObjects: number;
  ports: number[];
  connections: number;
}

export interface ProcessDetail {
  key: string;
  cmd: string;
  cwd: string;
  exe: string;
  description: string | null;
  publisher: string | null;
}

export interface AppSummary {
  label: string;
  category: Category;
  processCount: number;
  instanceCount: number;
  private: number;
  workingSet: number;
  privateWs: number;
  cpu: number;
  handles: number;
  threads: number;
  trend: number[];
}

export interface PortInfo {
  port: number;
  address: string;
  exposed: boolean;
  pid: number;
  key: string | null;
  processName: string;
  role: string;
  tech: string | null;
  appLabel: string;
  project: Project | null;
  category: Category;
  connections: number;
  private: number;
  uptimeSecs: number;
}

export interface ServiceInfo {
  name: string;
  displayName: string;
  pid: number;
  key: string | null;
}

export interface SystemInfo {
  totalMemory: number;
  usedMemory: number;
  availableMemory: number;
  commitTotal: number;
  commitLimit: number;
  systemCache: number;
  kernelPaged: number;
  kernelNonpaged: number;
  cpu: number;
  cpuCount: number;
  processCount: number;
  threadCount: number;
  handleCount: number;
  uptimeSecs: number;
  limitedProcesses: number;
  elevated: boolean;
  hostName: string;
  platform: string;
  windowsKnown: boolean;
}

export type Metric = "memory" | "handles" | "threads" | "gdi" | "userObjects" | "nonpagedPool";
export type Confidence = "plateaued" | "watching" | "low" | "medium" | "high";
export type Pattern = "steadyClimb" | "stairStep" | "sawtoothRisingFloor" | "plateau";

export interface LeakReport {
  id: string;
  subject: "process" | "app" | "system";
  key: string | null;
  name: string;
  role: string;
  appLabel: string;
  project: Project | null;
  metric: Metric;
  confidence: Confidence;
  pattern: Pattern;
  ratePerHour: number;
  growth: number;
  growthPct: number;
  startValue: number;
  currentValue: number;
  observedSecs: number;
  zScore: number;
  monotonicity: number;
  hoursToExhaustion: number | null;
  series: [number, number][];
}

export type Severity = "info" | "warning" | "critical";

export interface Insight {
  id: string;
  severity: Severity;
  title: string;
  detail: string;
  action: string;
  reclaimable: number;
  keys: string[];
}

export interface ExitedInfo {
  key: string;
  pid: number;
  name: string;
  role: string;
  appLabel: string;
  project: Project | null;
  category: Category;
  started: number;
  startKnown: boolean;
  ended: number;
  peakPrivate: number;
  identity: string;
}

export interface SelfStats {
  private: number;
  cpu: number;
  sampleMs: number;
  trackedSeries: number;
}

export interface Snapshot {
  seq: number;
  t: number;
  monitorSince: number;
  system: SystemInfo;
  processes: ProcessInfo[];
  apps: AppSummary[];
  ports: PortInfo[];
  services: ServiceInfo[];
  leaks: LeakReport[];
  insights: Insight[];
  exited: ExitedInfo[];
  selfStats: SelfStats;
}

export interface Settings {
  intervalSecs: number;
  bucketSecs: number;
  leakMinMinutes: number;
  leakWarmupMinutes: number;
  notifyLeaks: boolean;
  notifyPressure: boolean;
  pressurePct: number;
  closeToTray: boolean;
  ignored: string[];
}

/** One history sample. Memory values are KiB. */
export interface Bucket {
  t: number;
  p: number;
  pm: number;
  px: number;
  w: number;
  h: number;
  hm: number;
  n: number;
  nm: number;
  g: number;
  gm: number;
  u: number;
  um: number;
  c: number;
  k: number;
}

export interface SysBucket {
  t: number;
  used: number;
  commit: number;
  nonpaged: number;
  nonpaged_min: number;
  cache: number;
  cpu: number;
  k: number;
}

export interface ProcessHistory {
  fine: Bucket[];
  lifetime: Bucket[];
  span: number;
}

export interface SystemHistory {
  fine: SysBucket[];
  lifetime: SysBucket[];
}

export interface KillOutcome {
  killed: number[];
  failed: { pid: number; name: string; reason: string }[];
}
