# Contributing to Wraith

Wraith is a Windows process and memory leak monitor built with Tauri 2, Rust, Preact and Tailwind CSS v4. Contributions from people and AI agents are welcome. Agents must also follow [AGENTS.md](AGENTS.md).

## How changes get merged

- Every change goes through a pull request. Nothing is pushed directly to `main`.
- Every pull request needs approval from the maintainer, [@mcclaskiem](https://github.com/mcclaskiem), who is the only person who merges.
- Pull requests are squash merged, so the title becomes the commit message on `main`.

## Before you start

Check the [roadmap](ROADMAP.md) and the open issues. For anything large, or anything not on the roadmap, open an issue first so you don't build something that won't be accepted.

Keep each pull request to one change. A bug fix and a refactor are two pull requests.

## Setup

You need Windows 10 22H2 or later, Node 24, the Rust stable MSVC toolchain, Visual Studio Build Tools and WebView2.

```bash
npm install
```

```bash
npm run app:dev
```

Dev builds run without administrator rights. Release builds require them, so `npm run app:build` produces an app that shows a UAC prompt at launch.

If the build fails with `LNK1104`, your project path is too long for Windows. Move the project somewhere shorter, like `C:\Development\wraith`.

## Checks

All of these must pass before you ask for review:

```bash
npm run typecheck
```

```bash
npm test
```

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib
```

```bash
cargo clippy --manifest-path src-tauri/Cargo.toml --lib --tests
```

Tests can't catch everything here, because Wraith depends on real processes. If your change affects what the app shows or does, also run it and check the result by hand.

To see classification results on your own machine, write a snapshot to a file outside the repo and read it. PowerShell:

```powershell
$env:WRAITH_DUMP = "$env:TEMP\wraith-dump.json"; cargo test --manifest-path src-tauri/Cargo.toml --lib dump_live -- --ignored --nocapture
```

To exercise leak detection, run a deliberate leak (megabytes per second, then seconds to run), and lower the time windows in Settings:

```bash
node scripts/leak-test.mjs 1 480
```

## Where things live

| To change | Edit | Add a test in |
| --- | --- | --- |
| How a process is identified | `src-tauri/src/classify.rs` | the same file |
| Which app or project it's tied to | `src-tauri/src/relate.rs` | the same file |
| Leak detection | `src-tauri/src/leaks.rs` | the same file |
| Cleanup suggestions | `src-tauri/src/insights.rs` | the same file |
| What ending a process does | `src/impact.ts` | `src/impact.test.ts` |
| Process table grouping and search | `src/tree.ts` | `src/tree.test.ts` |
| A screen | `src/views/` | |

Types shared with the UI live in `src-tauri/src/model.rs` and must be mirrored in `src/types.ts`.

## Writing the code

- Match the code around your change, including comment density and naming.
- Comment why something is done, not what it does.
- Every `unsafe` block needs a `SAFETY` comment.
- Behaviour changes need a test. Bug fixes need a test that fails without the fix.
- Avoid new dependencies. If you need one, explain why in the pull request.

## Writing the interface

- Keep text short and plain. Prefer a label, a chip or a table column to a sentence, and a tooltip or expandable row to a paragraph.
- Never show a number or claim Wraith can't back up. If a value is unknown, show a dash.
- Status colors always come with an icon and a label.
- Use the color tokens in `src/styles.css`, and check both the light and dark themes.
- Charts use one axis each, a legend when there are two or more series, and a hover readout.

## Don't publish your machine's data

Process lists contain project names, paths, usernames and command lines, and command lines can contain secrets. This is a public repo, so check every screenshot, log and test fixture you share. Use made-up names like `C:\Users\me\...` in tests.

## Pull requests

Fill in the template. Say what changed, why, and how you checked it, and include a screenshot for visual changes. Use a short title in the imperative, like `Fix uptime for elevated processes`.

## License

By contributing, you agree that your work is licensed under the [MIT License](LICENSE).
