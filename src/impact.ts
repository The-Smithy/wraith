import { shortRole } from "./format";
import type { ProcessInfo } from "./types";

/** Ordered from least to most disruptive, with `blocked` meaning it cannot be ended here. */
export type Risk = "restarts" | "safe" | "caution" | "high" | "blocked";

export interface Impact {
  risk: Risk;
  /** A few words, for table cells. */
  label: string;
  /** Short facts, for the details panel and the confirmation. */
  effects: string[];
  /** How to get back to where you were. */
  recovery: string;
}

function impact(risk: Risk, label: string, effects: string[], recovery: string): Impact {
  return { risk, label, effects, recovery };
}

// Chromium restarts these itself. Unnamed helpers are left out, since that can't be promised.
const HELPERS = /^(GPU process|Network service|Storage service|Audio service|Video capture service)$/;

function chromiumPart(p: ProcessInfo, role: string, what: "editor" | "browser" | "app"): Impact | null {
  const app = p.appLabel;
  if (role === "Main process") {
    return what === "browser"
      ? impact("high", `Closes ${app}`, ["All windows and tabs close", "Unsaved form data is lost"], "Reopen it. Tabs usually restore.")
      : impact("high", `Closes ${app}`, ["All its windows close", "Everything running inside it stops", "Unsaved work may be lost"], "Reopen it. It usually restores your windows.");
  }
  if (HELPERS.test(role)) {
    return impact("restarts", "Restarts itself", [`${app} may flicker for a moment`], `${app} starts a new one on its own.`);
  }
  if (role === "Crash reporter") {
    return impact("safe", "Stops crash reports", [`${app} stops reporting crashes for now`], `It returns when ${app} restarts.`);
  }
  if (role === "Tab or extension") {
    return impact("caution", "Crashes tabs", ["A tab or extension crashes", "Unsaved form data there is lost"], "Reload the tab.");
  }
  if (role === "Window or webview" || role === "Web content") {
    return impact("high", "Crashes a window", ["A window or panel crashes", "Unsaved work there may be lost"], `Reopen it from ${app}.`);
  }
  return null;
}

function editorPart(p: ProcessInfo, role: string): Impact {
  const part = chromiumPart(p, role, "editor");
  if (part) return part;
  if (role === "Extension host") {
    return impact(
      "caution",
      "Stops extensions",
      ["Extensions and language servers in this window stop", "Claude Code sessions in this window end", "The window stays open"],
      `${p.appLabel} offers to restart extensions.`,
    );
  }
  if (role === "Terminal host") {
    return impact("caution", "Closes terminals", ["Terminals in the editor close", "Anything running in them may stop"], "Open a new terminal and rerun it.");
  }
  return impact("caution", "Editor may glitch", ["A background part of the editor stops"], "Reload the window if something breaks.");
}

function appChild(p: ProcessInfo, role: string): Impact {
  const part = chromiumPart(p, role, "app");
  if (part) return part;
  return impact("caution", "May break app", [`${p.appLabel} may crash or lose a feature`], `Restart ${p.appLabel} if it misbehaves.`);
}

const TERMINAL_APPS = /^(Windows Terminal|GNOME Terminal|Ptyxis|Konsole|xterm|kitty|foot|Tilix|Terminator|Xfce Terminal|Alacritty|WezTerm)$/;

function terminal(p: ProcessInfo, role: string): Impact {
  if (role.startsWith("Shell command")) {
    return impact("safe", "Stops command", ["This one command stops"], "Run it again.");
  }
  if (role === "Console host") {
    return impact("caution", "Closes a console", ["The program attached to it may exit"], "Start the program again.");
  }
  if (role === "Terminal multiplexer") {
    return impact("high", "Ends all sessions", ["Every session in it closes", "Anything running in them stops"], "Start a new session and rerun things.");
  }
  if (p.presence === "window" || TERMINAL_APPS.test(role)) {
    return impact("high", "Closes terminal", ["The window and its tabs close", "Anything running in them may stop"], "Open it again.");
  }
  return impact("caution", "Closes shell", ["This shell closes", "Programs it started may keep running"], "Open a new shell.");
}

/** The consequence of ending one process, ignoring its children. */
function ownImpact(p: ProcessInfo): Impact {
  const role = shortRole(p);

  if (p.name.toLowerCase().replace(/\.exe$/, "") === "wraith") {
    return impact("blocked", "Is Wraith", ["This is Wraith itself"], "Quit from the tray icon.");
  }
  if (p.protected || p.category === "system") {
    return impact("blocked", "Protected", ["Part of the operating system, so Wraith will not end it", "Ending it could crash or sign out your PC"], "Nothing to do. The system manages it.");
  }
  if (p.limited) {
    return impact("blocked", "Access denied", ["The system will not let Wraith end this"], "Some processes are locked even for administrators.");
  }
  if (p.services.length > 0) {
    const named = p.services.slice(0, 3).join(", ");
    const more = p.services.length > 3 ? ` and ${p.services.length - 3} more` : "";
    return impact(
      "high",
      p.services.length === 1 ? "Stops a service" : `Stops ${p.services.length} services`,
      [`Stops ${named}${more}`, "Features that rely on it may break", "It may restart on its own"],
      "Restart it from your service manager or reboot.",
    );
  }

  switch (p.category) {
    case "editor":
      return editorPart(p, role);
    case "browser":
      // HELPERS describes Chromium, so it says nothing about what WebKit restarts.
      if (p.tech === "WebKit" || (p.tech === "WebView2" && !HELPERS.test(role))) {
        return impact("caution", "Breaks a web view", [`The web view inside ${p.appLabel} stops`, `${p.appLabel} may crash`], `Restart ${p.appLabel} if it misbehaves.`);
      }
      return chromiumPart(p, role, "browser") ?? impact("caution", "Browser may glitch", ["A background part of the browser stops"], "Restart the browser if something breaks.");
    case "languageServer":
      return impact("restarts", "Restarts itself", ["Autocomplete and error checking pause briefly"], "The editor usually starts a new one, using less memory.");
    case "aiAgent":
      if (role.startsWith("Claude Code") || role.startsWith("Codex") || role.startsWith("Gemini")) {
        return impact("caution", "Ends the session", ["The session stops mid-task", "Commands it started may keep running"], "You can usually resume the conversation.");
      }
      return chromiumPart(p, role, "app") ?? impact("caution", "Stops AI tool", ["The assistant stops working"], "Restart the app that runs it.");
    case "devServer":
      return impact(
        "safe",
        p.ports.length ? `:${p.ports[0]} goes offline` : "Stops server",
        [p.ports.length ? `localhost:${p.ports[0]} stops responding` : "The server stops", "Anything held only in its memory is lost"],
        "Run your dev command again.",
      );
    case "buildTool":
      return impact("safe", "Stops build", ["The current build or watcher stops"], "Run it again.");
    case "testRunner":
      return impact("safe", "Stops tests", ["The current test run stops"], "Run the tests again.");
    case "mcpServer":
      return impact("safe", "AI tool stops", ["The connected AI agent loses this tool"], "Restart the agent to reconnect it.");
    case "packageManager":
      return impact("caution", "Stops script", ["The running script stops", "An install may be left half finished"], "Run it again.");
    case "runtime":
      return impact("caution", "Stops script", ["The script stops", "Anything it has not saved is lost"], "Run it again.");
    case "database":
      return impact("high", "Drops connections", ["Connected apps lose their database", "Unwritten data may be lost"], "Prefer stopping it with its own command.");
    case "container":
      return impact("high", "Stops containers", ["Containers or WSL distros stop", "Work inside them may be lost"], "Prefer docker stop or wsl --shutdown.");
    case "terminal":
      return terminal(p, role);
    case "versionControl":
      return impact("caution", "Stops Git", ["The operation stops partway", "It may leave a lock file behind"], "Delete .git/index.lock if Git complains.");
    case "app":
      // Wayland can't say if it has a window, so don't call it safe.
      if (p.presence === "unknown") {
        return impact("caution", "Stops app", [`${role} stops`, "Any open windows close, losing unsaved work"], "Open it again.");
      }
      if (p.presence === "window" || p.key === p.appKey) {
        return p.presence === "window"
          ? impact("high", `Closes ${role}`, [`${role} closes right away`, "Unsaved work is lost"], "Open it again.")
          : impact("safe", "Stops until relaunch", ["This background app stops", "Its tray icon or features go away"], "It returns next launch or sign-in.");
      }
      return appChild(p, role);
    default:
      return impact("caution", "Effect unknown", ["Wraith does not recognise this process"], "Search for its name before ending it.");
  }
}

/** How many real processes each process started. Console hosts come and go with their program. */
export function childCounts(processes: ProcessInfo[]): Map<string, number> {
  const counts = new Map<string, number>();
  for (const p of processes) {
    if (p.parentKey && p.name.toLowerCase() !== "conhost.exe") {
      counts.set(p.parentKey, (counts.get(p.parentKey) ?? 0) + 1);
    }
  }
  return counts;
}

/** What ending `p` will do. `childCount` is how many processes it started directly. */
export function endImpact(p: ProcessInfo, childCount = 0): Impact {
  const own = ownImpact(p);
  // Windows does not end children with their parent, except where an app tears down its own.
  const selfManaged = p.category === "editor" || p.category === "browser" || p.category === "languageServer" || own.risk === "blocked";
  if (childCount > 0 && !selfManaged) {
    const noun = childCount === 1 ? "1 process it started" : `${childCount} processes it started`;
    return { ...own, effects: [...own.effects, `${noun} may keep running`] };
  }
  return own;
}
