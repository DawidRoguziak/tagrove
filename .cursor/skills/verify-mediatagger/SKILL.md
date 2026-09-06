---
name: verify-mediatagger
description: Launch and control the real MediaTagger Tauri desktop on a private Xvfb display using disposable E2E media. Use for UI clicks, typing, screenshots, console inspection, and desktop behavior verification.
---

# Verify MediaTagger

Read [the feature map](features/README.md) and the relevant recipe before driving the app.
This controls a real Tauri E2E build with SQLite and native media rendering. No browser mocks.

## Launch

Run from the repository root on Linux. Required tools are Bun, Node, the normal Tauri build
dependencies, Xvfb, xauth, ImageMagick `import`, tauri-driver, WebKitWebDriver, and ffmpeg/ffprobe.
Honor `TAURI_DRIVER_PATH`, `WEBKIT_WEBDRIVER_PATH`, and `FFMPEG_PATH` when set.
Use the existing normal setup instructions for missing dependencies. Do not change app code
to work around an unavailable display, driver, or graphics stack.

```sh
bun run app:control start
```

Keep this foreground command alive in a terminal or a tool exec session. Build output goes
to the printed artifact path. Wait for `READY`, which includes the session ID, E2E title,
temporary app-data path, private display, and process IDs. In a second terminal/tool call,
set `control_session` to that returned ID. This is a shell variable, not a literal placeholder.
The examples below use it.

The controller builds fresh frontend assets and the existing E2E binary, selects English,
and indexes four copied PNGs plus the E2E GIF and two MP4 fixtures. It owns fresh temporary
XDG data/config/cache directories. It uses local ports 4446 and 4447, an authenticated Xvfb
display, and a private Unix socket. Local sockets and native desktop processes may require
a tool sandbox escalation. Keep all fixture paths inside the returned temporary media root.

One controller per checkout. Do not run another E2E build/suite concurrently because they
share `dist/` and the E2E build target. A lock prevents duplicate controllers. Never create
a worktree. Do not attach to a normal dev or production instance.

## Doctor

```sh
bun run app:control doctor "$control_session"
```

Require the exact title `Tagrove E2E`, app data below the run's temporary root,
only the seeded media scan root, live owned display/driver processes, and a responsive UI.
Run this first when anything looks wrong. Interaction commands repeat these checks.
Read `session.json` and `process.log` in the artifact directory if startup fails.
An occupied port is an error; never kill its owner to make this session start.

## Drive

```sh
bun run app:control inspect "$control_session"
bun run app:control fill "$control_session" .filter-input absent-control-tag
bun run app:control keys "$control_session" Enter
bun run app:control inspect "$control_session" .filter-input
bun run app:control click "$control_session" 'button[aria-label="Clear all search filters"]'
bun run app:control click "$control_session" 'button[aria-label="Open settings"]'
bun run app:control inspect "$control_session" h2
bun run app:control click "$control_session" 'button[aria-label="Back"]'
```

`click`, `fill`, and `scroll` take WebdriverIO selectors. `inspect` takes an optional CSS
selector and returns visible text, values, and element attributes. `keys` accepts WebdriverIO
key names; multiple arguments describe a key combination. `scroll <selector>` brings a DOM
element into view. Commands serialize through one persistent browser session. Inspect the
result of an action before assuming the UI finished its asynchronous work.

Use `refresh` for a controlled reload. It records the console gap and reattaches collection.
No arbitrary JavaScript or IPC command is exposed. Native GTK video controls and native file
dialogs have no DOM selectors and cannot be clicked through this controller. Report those
paths as unsupported rather than substituting internal application setters.

## Evidence

```sh
bun run app:control screenshot "$control_session"
bun run app:control logs "$control_session"
bun run app:control diagnose "$control_session"
bun run app:control logs "$control_session"
```

Open the returned PNG with the agent's image viewer. Screenshots capture only the private
display, including native GTK overlays. A successful screenshot command does not prove
the media rendered: inspect its pixels. On the verified host, PNG/gallery screenshots work;
the MP4 picture area was black or corrupt on Xvfb, including with software GL. Native video
pixels remain unverified. Keep playback changes outside this control tooling task.

Artifacts survive under `artifacts/app-control/<session>/`:

- `screen-*.png`: private-display screenshots.
- `actions.jsonl`: requested actions, results, inspections, failures, and timestamps.
- `console.jsonl`: frontend console output, window errors, unhandled rejections, and capture gaps.
- `process.log`: prerequisite checks, builds, drivers, and inherited application stdout/stderr.
- `session.json`: session identity, process groups, temporary paths, and lifecycle status.

`logs` returns the last 24,000 characters per log; full logs remain on disk. Frontend capture
begins after WebDriver attaches and is reinstalled after reloads. It cannot recover startup
messages, worker consoles, or messages lost in a reload gap. Console calls still invoke the
original console functions. The bounded in-page buffer reports dropped entries.
`diagnose` emits identifiable info/error messages and a synthetic window error to test the
collector; it is diagnostic evidence, not a test of application error handling.

Prove user behavior through real clicks and typing, capture the resulting UI, and verify
persisted effects when a feature writes data. Use read-only inspection of the temporary
SQLite database or close/reopen the affected UI. Do not claim a screenshot alone proves a
database mutation. Do not use repository icons themselves as a deletable scan root.

## Cleanup

```sh
bun run app:control stop "$control_session"
bun run app:control stop "$control_session"
```

Stop closes the WebDriver session and owned process groups, then removes only the owned
temporary directory. It leaves screenshots and logs intact. Repeated stop is safe. SIGINT,
SIGTERM, and SIGHUP also clean up. Verify `session.json` says `stopped`, its temporary root
is gone, and evidence still exists. If a process will not stop, preserve the temporary data.

SIGKILL or a host crash cannot run cleanup. A leftover `active.lock` requires manual
inspection of its recorded controller and the manifest's process groups before recovery.
Never kill by name or delete a lock just because a client cannot connect. After proving
the recorded processes are gone, the exported `removeOwnedScratch(root, id)` helper in
`e2e/control-support.js` checks the ownership marker before removal.

## Helpers

The executable helper is `e2e/control.js`, exposed as `bun run app:control`; `help` lists commands.
Run `bun run test:app-control` for socket, cleanup, process-tree, input, and fixture checks.
Keep feature recipes current when selectors or behavior change.
