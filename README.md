# Wraith

A Windows desktop monitor for every process on your PC, built for web developers. It shows what each process really is, what it is tied to, and whether it is leaking.

Built with Tauri 2, Rust, Preact and Tailwind CSS v4. Windows only.

## What it does

- **Names what generic processes really are.** `node.exe` becomes Vite, Next.js or an MCP server. `Code.exe` becomes the extension host, Pylance or the TypeScript server.
- **Ties every process to an app:** the app, its publisher, the parent chain and the project, found from the working folder or script path. An app with its own window from a different publisher counts as its own app, so a game isn't filed under Steam.
- **Says what ending a process will do:** every row has an "If ended" label, such as "Restarts itself" or ":3000 goes offline", and the details panel lists the effects and how to recover.
- **Finds leaks in memory, handles, threads, and GDI and USER objects.** It also checks app totals and the kernel non-paged pool.
- **Maps ports to projects,** with the clients connected and whether a port is open to your whole network.
- **Suggests what to clean up:** forgotten dev servers, duplicate servers for one project, duplicate language servers, heavy background apps and crash loops.
- **Tracks background work:** Windows services, windowless apps and recently exited processes.
- **Keeps running in the tray,** so it can gather long stretches of data, and sends a notification for likely leaks and low RAM.

## Run it

```bash
npm install
```

```bash
npm run app:dev
```

Build a standalone app and installer:

```bash
npm run app:build
```

Release builds require administrator, so Windows shows a UAC prompt at launch. That lets Wraith read every process it is allowed to. Development builds (`npm run app:dev`) run without admin so they can start from a normal terminal, and elevated processes show a dash for memory, handles and uptime there.

Windows components, including service hosts like `svchost.exe`, are always protected from being ended. Third party services can be ended, with a warning.

## How leak detection works

Memory going up is not enough evidence, since garbage collected heaps swing up and down, caches fill, and everything grows at startup. For each process, Wraith:

1. Skips a startup grace period, 10 minutes by default.
2. Splits the history into equal stretches of time and takes the lowest value in each. That floor is what survives garbage collection.
3. Tests whether the floor keeps rising with the Mann-Kendall trend test, and measures the rate with the Theil-Sen slope, which one spike cannot skew.
4. Requires enough growth to matter, and checks the growth is still happening at the end of the window, so a cache that filled up is reported as "Levelled off" instead.

| Confidence | What it takes, at default settings |
| --- | --- |
| Low | 30 minutes of data, significant trend, growth above the minimum |
| Medium | 2 hours of data and a stronger trend |
| High | 2 hours of data, a very strong and mostly unbroken rise, and at least twice the minimum growth |

A flagged process is a suspect, not proof. Confirm with the tools for its language, such as a heap snapshot.

To see the detector work, run a deliberate leak (megabytes per second, then seconds to run):

```bash
node scripts/leak-test.mjs 1 3600
```

## History and memory use

Each series holds at most 480 points. When one fills up, neighbouring points merge and the time span of each point doubles, so a process's whole lifetime stays covered in bounded memory.

History and settings live in `%LOCALAPPDATA%\Wraith`, and history is saved every 5 minutes and on quit. Full command lines stay in memory only and are never written to disk.

## Project layout

| Path | What it holds |
| --- | --- |
| `src-tauri/src/sampler.rs` | The tick that collects data and runs the analyzers |
| `src-tauri/src/classify.rs` | Works out what a process is |
| `src-tauri/src/relate.rs` | Parent chains, owning apps and projects |
| `src-tauri/src/leaks.rs` | Leak statistics |
| `src-tauri/src/insights.rs` | Optimization rules |
| `src-tauri/src/history.rs` | Bounded series and persistence |
| `src-tauri/src/win.rs` | Win32 calls |
| `src/impact.ts` | What ending each kind of process does |
| `src/views/` | One file per screen |

## Tests

```bash
npm test
```

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib
```

## Contributing

See the [contributing guide](CONTRIBUTING.md) and the [roadmap](ROADMAP.md). AI agents must also follow [AGENTS.md](AGENTS.md). Every pull request needs the maintainer's approval.

## License

[MIT](LICENSE)
