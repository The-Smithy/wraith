import { describe, expect, it } from "vitest";
import { bytes, duration } from "./format";
import { buildRows, chainOf } from "./tree";
import type { ProcessInfo } from "./types";

function proc(key: string, over: Partial<ProcessInfo> = {}): ProcessInfo {
  return {
    key,
    pid: Number(key),
    parentKey: null,
    parentGone: false,
    name: `p${key}.exe`,
    exe: "",
    cmdShort: "",
    startTime: 1000,
    startKnown: true,
    user: "",
    category: "app",
    protected: false,
    role: `Role ${key}`,
    tech: null,
    appLabel: "App",
    appKey: "1",
    project: null,
    services: [],
    windowTitle: null,
    presence: "child",
    limited: false,
    private: 100,
    workingSet: 50,
    privateWs: 50,
    cpu: 0,
    readRate: 0,
    writeRate: 0,
    handles: 1,
    threads: 1,
    gdi: 0,
    userObjects: 0,
    ports: [],
    connections: 0,
    ...over,
  };
}

const base = { mode: "apps", sort: "private", filter: "all", query: "", toggled: new Set<string>() } as const;

describe("buildRows", () => {
  const list = [
    proc("1"),
    proc("2", { parentKey: "1", private: 300 }),
    proc("3", { parentKey: "2", private: 500 }),
    proc("4", { appLabel: "Other", private: 2000 }),
  ];

  it("groups by app, closed by default, largest first", () => {
    const rows = buildRows(list, base);
    expect(rows.map((r) => r.label)).toEqual(["Other", "App"]);
    expect(rows[1].totals.private).toBe(900);
    expect(rows[1].expanded).toBe(false);
  });

  it("opens an app into its process tree with depth", () => {
    const rows = buildRows(list, { ...base, toggled: new Set(["app:App"]) });
    const app = rows.filter((r) => r.process && r.process.appLabel === "App");
    expect(app.map((r) => [r.id, r.depth])).toEqual([
      ["1", 1],
      ["2", 2],
      ["3", 3],
    ]);
  });

  it("a collapsed process row reports its whole subtree", () => {
    const rows = buildRows(list, { ...base, toggled: new Set(["app:App", "2"]) });
    const collapsed = rows.find((r) => r.id === "2")!;
    expect(collapsed.expanded).toBe(false);
    expect(collapsed.totals.private).toBe(800);
    expect(rows.find((r) => r.id === "3")).toBeUndefined();
  });

  it("searching opens groups and survives a missing parent", () => {
    const rows = buildRows(list, { ...base, query: "role 3" });
    expect(rows.map((r) => r.id)).toEqual(["app:App", "3"]);
  });

  it("finds a process by its port", () => {
    const rows = buildRows([proc("9", { ports: [5173] })], { ...base, mode: "flat", query: "5173" });
    expect(rows).toHaveLength(1);
  });

  it("flat mode can group rows by their app", () => {
    const rows = buildRows(list, { ...base, mode: "flat", sort: "app" });
    expect(rows.map((r) => r.process!.appLabel)).toEqual(["App", "App", "App", "Other"]);
    expect(rows[0].id).toBe("3");
  });

  it("flat mode sorts every process together", () => {
    const rows = buildRows(list, { ...base, mode: "flat" });
    expect(rows.map((r) => r.id)).toEqual(["4", "3", "2", "1"]);
  });
});

describe("chainOf", () => {
  it("walks to the root and stops at a missing parent", () => {
    const a = proc("1");
    const b = proc("2", { parentKey: "1" });
    const c = proc("3", { parentKey: "2" });
    const d = proc("4", { parentKey: "gone" });
    const map = new Map([a, b, c, d].map((p) => [p.key, p]));
    expect(chainOf(c, map).map((p) => p.key)).toEqual(["1", "2", "3"]);
    expect(chainOf(d, map).map((p) => p.key)).toEqual(["4"]);
  });

  it("does not hang on a cycle", () => {
    const a = proc("1", { parentKey: "2" });
    const b = proc("2", { parentKey: "1" });
    const map = new Map([a, b].map((p) => [p.key, p]));
    expect(chainOf(a, map).length).toBeLessThan(30);
  });
});

describe("format", () => {
  it("formats sizes and durations", () => {
    expect(bytes(1536 * 1024 * 1024)).toBe("1.5 GB");
    expect(bytes(5 * 1024 * 1024)).toBe("5 MB");
    expect(duration(45)).toBe("45s");
    expect(duration(3700)).toBe("1h 1m");
    expect(duration(93600)).toBe("1d 2h");
  });
});
