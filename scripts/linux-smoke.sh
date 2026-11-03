#!/usr/bin/env bash
# Runs the debug build in a virtual desktop and checks what it reports about real
# processes. Used by CI, and needs an X server and window manager to be available.
set -u
out="$PWD/smoke"
mkdir -p "$out"

openbox &
sleep 1
python3 -m http.server 8799 --bind 127.0.0.1 >/dev/null 2>&1 &
server=$!

export WRAITH_DATA_DIR="$out/data"
./src-tauri/target/debug/wraith >"$out/app.log" 2>&1 &
app=$!

status=0

# Wait for the page itself, because a window can exist with nothing drawn in it. A desktop
# with an empty window has a few dozen colours, and a page with text has hundreds.
needed=100
colours=0
started=$SECONDS
while [ $((SECONDS - started)) -lt 120 ]; do
  import -window root "$out/screenshot.png" 2>/dev/null
  colours=$(identify -format %k "$out/screenshot.png" 2>/dev/null)
  colours=${colours:-0}
  [ "$colours" -ge "$needed" ] && break
  sleep 3
done
if [ "$colours" -ge "$needed" ]; then
  echo "ok     the page drew after $((SECONDS - started))s ($colours colours)"
else
  echo "FAILED the page never drew ($colours colours after $((SECONDS - started))s)"
  status=1
fi

# Give it a few samples so the screenshot shows real numbers.
sleep 10
kill -0 "$app" 2>/dev/null || { echo "FAILED the app exited early"; status=1; }
import -window root "$out/screenshot.png" || echo "could not take a screenshot"

WRAITH_DUMP="$out/snapshot.json" cargo test --manifest-path src-tauri/Cargo.toml --lib dump_live -- --ignored --nocapture 2>&1 | grep -E "sample took|test result" || true

node - "$out/snapshot.json" "$server" "$app" <<'EOF' || status=1
const { readFileSync } = require("node:fs");
const [file, server, app] = process.argv.slice(2);
const s = JSON.parse(readFileSync(file, "utf8"));
const fails = [];
const check = (name, ok, detail = "") => {
  console.log(`${ok ? "ok    " : "FAILED"} ${name} ${detail}`);
  if (!ok) fails.push(name);
};
const byPid = (pid) => s.processes.find((p) => p.pid === Number(pid));

check("reports the platform", s.system.platform === "linux", s.system.platform);
check("sees many processes", s.processes.length > 20, `${s.processes.length}`);
check("no thread entries", new Set(s.processes.map((p) => p.pid)).size === s.processes.length);
check("memory figures are readable", s.system.totalMemory > 0 && s.system.kernelNonpaged > 0);
check("commit limit is not a false alarm", !s.insights.some((i) => i.id === "pressure:commit"));

const web = byPid(server);
check("finds the test server", Boolean(web));
if (web) {
  check("names the test server", web.role === "Python HTTP server", web.role);
  check("counts it as a dev server", web.category === "devServer", web.category);
  check("reads its private memory", web.memoryKnown && web.private > 0, `${web.private}`);
  check("reads its handles", !web.limited && web.handles > 0, `${web.handles}`);
  check("knows its start time", web.startKnown && web.startTime > 1e9);
  check("ties it to the port", web.ports.includes(8799), web.ports.join(","));
}
const port = s.ports.find((p) => p.port === 8799);
check("port table names the owner", port?.key === web?.key && port?.exposed === false);

const wraith = byPid(app);
check("finds Wraith", Boolean(wraith));
check("detects Wraith's window through X11", wraith?.presence === "window", `${wraith?.presence} ${wraith?.windowTitle}`);

check("kernel threads are protected", s.processes.filter((p) => p.role === "Kernel thread").every((p) => p.protected));
const init = byPid(1);
check("process 1 is protected", init?.protected === true, init?.name);

const roles = {};
for (const p of s.processes) roles[p.category] = (roles[p.category] ?? 0) + 1;
console.log("categories:", JSON.stringify(roles));
console.log("generic rows:", s.processes.filter((p) => p.category === "other").map((p) => p.name).join(", ") || "none");
console.log("apps:", s.apps.slice(0, 12).map((a) => `${a.label} (${a.processCount})`).join(", "));
console.log("insights:", s.insights.map((i) => i.title).join(" | ") || "none");
process.exit(fails.length ? 1 : 0);
EOF

echo "--- app log ---"
cat "$out/app.log"
kill "$app" "$server" 2>/dev/null
exit $status
