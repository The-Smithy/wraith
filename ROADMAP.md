# Roadmap

This is the order we intend to work in, not a set of promises or dates. Each item should be its own pull request. To suggest a change, open an issue.

## 1. Verify and harden what exists

These come first, because the rest of the app depends on them being right.

- [x] Add CI that runs the typecheck, tests and clippy on every pull request, on Windows and Linux
- [ ] Test a real elevated run: the admin status, protected processes and Show in Explorer
- [ ] Test desktop notifications, the tray's Quit item and saving history on quit
- [ ] Test the installer, including "run after install" with the UAC prompt
- [ ] Measure Wraith's own memory over a multi-day run, and fix any growth
- [ ] Check the "If ended" explanations against what really happens for each kind of process
- [ ] Check leak detection at default settings against long-running real apps, and record false positives

## 2. Finish Linux, then macOS

- [x] Linux support, tested in CI on a virtual desktop
- [ ] Use it on real Linux desktops, on both X11 and Wayland, and fix what turns up
- [ ] Check `If ended` explanations and system process rules on real Linux
- [ ] Detect windows on Wayland where the desktop allows it
- [ ] macOS support, once someone can test it on a real Mac

## 3. Ship it

- [ ] Publish versioned releases from CI
- [ ] Sign the installer so SmartScreen doesn't warn
- [ ] Publish Linux packages as well as the Windows installer
- [ ] Add screenshots to the README, using made-up data

## 4. See more

- [ ] Read memory for every process without opening it, so memory numbers don't depend on admin rights
- [ ] Add scheduled tasks and startup apps to the Background screen
- [ ] Show UDP ports alongside TCP
- [ ] Match each VS Code window to its own processes
- [ ] Recognize more dev tools and editors

## 5. Act more safely

- [ ] Ask an app to close before forcing it on Windows, as Linux already does
- [ ] Stop third party services through the service manager instead of ending their process
- [ ] Let a dev server be restarted from Wraith

## 6. Help fix leaks, not just find them

- [ ] Take a heap snapshot of a suspected Node process
- [ ] Compare a process's memory before and after an action
- [ ] Export a leak report to share or attach to an issue
- [ ] Offer to start Wraith at sign-in, off by default

## Out of scope for now

- Sending any data off the machine, including telemetry.
- Ending processes automatically. A person always decides.
