import { describe, expect, it } from "vitest";
import { childCounts, endImpact } from "./impact";
import type { ProcessInfo } from "./types";

function proc(over: Partial<ProcessInfo>): ProcessInfo {
  return {
    key: "1",
    pid: 1,
    parentKey: null,
    parentGone: false,
    name: "x.exe",
    exe: "",
    cmdShort: "",
    startTime: 0,
    startKnown: true,
    user: "",
    category: "app",
    protected: false,
    role: "Thing",
    tech: null,
    appLabel: "Thing",
    appKey: "1",
    project: null,
    services: [],
    windowTitle: null,
    presence: "child",
    limited: false,
    memoryKnown: true,
    private: 0,
    workingSet: 0,
    privateWs: 0,
    cpu: 0,
    readRate: 0,
    writeRate: 0,
    handles: 0,
    threads: 0,
    gdi: 0,
    userObjects: 0,
    ports: [],
    connections: 0,
    ...over,
  };
}

const editor = { category: "editor", tech: "VS Code", appLabel: "VS Code", appKey: "root" } as const;

describe("endImpact", () => {
  it("refuses Windows components, admin-only processes and Wraith", () => {
    expect(endImpact(proc({ category: "system" })).label).toBe("Protected");
    expect(endImpact(proc({ limited: true })).label).toBe("Access denied");
    expect(endImpact(proc({ protected: true, category: "service", services: ["Windows Update"] })).label).toBe("Protected");
    expect(endImpact(proc({ name: "wraith.exe" })).risk).toBe("blocked");
  });

  it("explains each part of an editor differently", () => {
    expect(endImpact(proc({ ...editor, role: "VS Code: Main process" })).label).toBe("Closes VS Code");
    expect(endImpact(proc({ ...editor, role: "VS Code: Extension host" })).label).toBe("Stops extensions");
    expect(endImpact(proc({ ...editor, role: "VS Code: Terminal host" })).label).toBe("Closes terminals");
    expect(endImpact(proc({ ...editor, role: "VS Code: GPU process" })).risk).toBe("restarts");
    expect(endImpact(proc({ ...editor, role: "VS Code: Window or webview" })).risk).toBe("high");
  });

  it("treats language servers as restarting and names a dev server's port", () => {
    expect(endImpact(proc({ category: "languageServer", role: "Pylance language server" })).risk).toBe("restarts");
    const server = endImpact(proc({ category: "devServer", role: "Vite dev server", ports: [5173] }));
    expect(server.label).toBe(":5173 goes offline");
    expect(server.effects[0]).toContain("localhost:5173");
  });

  it("separates browser tabs from the whole browser", () => {
    const browser = { category: "browser", tech: "Brave", appLabel: "Brave" } as const;
    expect(endImpact(proc({ ...browser, role: "Brave: Tab or extension" })).label).toBe("Crashes tabs");
    expect(endImpact(proc({ ...browser, role: "Brave: Main process" })).label).toBe("Closes Brave");
  });

  it("names the app's own window, not whatever launched it", () => {
    const game = proc({ presence: "window", role: "Melvor Idle 2", appLabel: "Steam", appKey: "steam" });
    expect(endImpact(game).label).toBe("Closes Melvor Idle 2");
  });

  it("does not claim a web view closes the app around it", () => {
    const webview = { category: "browser", tech: "WebView2", appLabel: "VS Code" } as const;
    expect(endImpact(proc({ ...webview, role: "WebView2: Main process" })).label).toBe("Breaks a web view");
    expect(endImpact(proc({ ...webview, role: "WebView2: Web content" })).label).toBe("Breaks a web view");
    expect(endImpact(proc({ ...webview, role: "WebView2: GPU process" })).risk).toBe("restarts");
  });

  it("handles Claude desktop parts like any other app", () => {
    const claude = { category: "aiAgent", tech: "Claude", appLabel: "Claude" } as const;
    expect(endImpact(proc({ ...claude, role: "Claude: Main process" })).label).toBe("Closes Claude");
    expect(endImpact(proc({ ...claude, role: "Claude Code session" })).label).toBe("Ends the session");
  });

  it("keeps long app names out of the table label", () => {
    const worker = endImpact(proc({ appKey: "root", appLabel: "ASUS NodeJS Web Framework", role: "ASUS NodeJS Web Framework: Worker process" }));
    expect(worker.label).toBe("May break app");
    expect(worker.effects[0]).toContain("ASUS NodeJS Web Framework");
  });

  it("recognises Wraith on every platform", () => {
    expect(endImpact(proc({ name: "wraith" })).risk).toBe("blocked");
  });

  it("does not call an app safe when windows can't be detected", () => {
    const app = endImpact(proc({ presence: "unknown", role: "GIMP" }));
    expect(app.label).toBe("Stops app");
    expect(app.risk).toBe("caution");
  });

  it("explains terminals, multiplexers and shells differently", () => {
    const term = { category: "terminal" } as const;
    expect(endImpact(proc({ ...term, role: "GNOME Terminal", presence: "unknown" })).label).toBe("Closes terminal");
    expect(endImpact(proc({ ...term, role: "Terminal multiplexer" })).label).toBe("Ends all sessions");
    expect(endImpact(proc({ ...term, role: "Zsh" })).label).toBe("Closes shell");
  });

  it("treats WebKit like any other embedded web view", () => {
    const view = proc({ category: "browser", tech: "WebKit", appLabel: "Wraith", role: "WebKit: Web content" });
    expect(endImpact(view).label).toBe("Breaks a web view");
  });

  it("doesn't promise that WebKit restarts its helpers", () => {
    for (const role of ["WebKit: GPU process", "WebKit: Network process"]) {
      const helper = proc({ category: "browser", tech: "WebKit", appLabel: "Wraith", role });
      expect(endImpact(helper).label).toBe("Breaks a web view");
    }
  });

  it("names the services that would stop", () => {
    const one = endImpact(proc({ category: "service", services: ["Parsec"] }));
    expect(one.label).toBe("Stops a service");
    expect(one.effects[0]).toBe("Stops Parsec");
    expect(endImpact(proc({ category: "service", services: ["A", "B", "C", "D"] })).effects[0]).toBe("Stops A, B, C and 1 more");
  });

  it("separates an app's window, its background root and its helpers", () => {
    expect(endImpact(proc({ presence: "window", role: "Discord", appLabel: "Discord" })).label).toBe("Closes Discord");
    expect(endImpact(proc({ presence: "background" })).label).toBe("Stops until relaunch");
    const helper = endImpact(proc({ appKey: "root", appLabel: "Discord", role: "Discord: GPU process" }));
    expect(helper.risk).toBe("restarts");
  });

  it("promises a restart only for helpers known to restart", () => {
    const app = { appKey: "root", appLabel: "Adobe" } as const;
    expect(endImpact(proc({ ...app, role: "Adobe: Helper process" })).label).toBe("May break app");
    expect(endImpact(proc({ ...app, role: "Adobe: Crash reporter" })).label).toBe("Stops crash reports");
  });

  it("does not count console hosts as children", () => {
    const counts = childCounts([
      proc({ key: "a", parentKey: "p", name: "conhost.exe" }),
      proc({ key: "b", parentKey: "p", name: "node.exe" }),
    ]);
    expect(counts.get("p")).toBe(1);
  });

  it("warns that children can outlive their parent, except in self-managed apps", () => {
    expect(endImpact(proc({ category: "terminal", role: "PowerShell" }), 2).effects.at(-1)).toBe("2 processes it started may keep running");
    expect(endImpact(proc({ ...editor, role: "VS Code: Main process" }), 5).effects.join()).not.toContain("keep running");
  });
});
