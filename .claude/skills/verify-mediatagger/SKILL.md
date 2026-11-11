---
name: verify-mediatagger
description: Launch and control the real MediaTagger Tauri desktop on a private Xvfb display using disposable E2E media. Use for UI clicks, typing, screenshots, console inspection, and desktop behavior verification.
---

# Verify MediaTagger

Read [the feature map](features/README.md) and the relevant recipe before driving the app.
This controls a real Tauri E2E build with SQLite and native media rendering. No browser mocks.

## Launch

Run from the repository root on Linux. Required tools are Bun, Node, the normal Tauri build
dependencies, Xvfb, xauth, ImageMagick `import`, tauri-driver, WebKitWebDriver, and ffmpeg.
Honor `TAURI_DRIVER_PATH`, `WEBKIT_WEBDRIVER_PATH`, and `FFMPEG_PATH` when set.
Use the existing normal setup instructions for missing dependencies. Do not change app code
to work around an unavailable display, driver, or graphics stack.

```sh
bun run app:control start
```

In Claude Code, run this with the Bash tool and `run_in_background: true`; the controller
must stay alive for the whole session. Build output goes to the printed artifact path. Wait
for the `READY {json}` line in the background task's output (use Monitor with an until-loop
on that output instead of polling with sleep). It includes the session ID, E2E title, temporary
app-data path, private display, and process IDs. Every later command is a separate Bash call;
shell variables do not persist between calls, so substitute the literal session ID wherever
the examples below write `"$control_session"`.

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

Richer gestures (all validated in `e2e/control-support.js`; `help` lists the ranges):

```sh
bun run app:control wait "$control_session" 'button[data-asset-index="0"]'          # displayed, 15 s
bun run app:control wait "$control_session" '[data-testid="tile-thumb-loader"]' hidden 30000
bun run app:control wait "$control_session" idle 60000
bun run app:control modclick "$control_session" Shift 'button[data-asset-index="5"]'
bun run app:control modclick "$control_session" Control+Shift 'button[data-asset-index="9"]'
bun run app:control dblclick "$control_session" '[data-testid="tag-list-modal"] li button'
bun run app:control drag "$control_session" 'button[data-asset-index="0"]' 40,260 200
bun run app:control drag "$control_session" '[data-testid="gallery-timeline"]' '[data-testid="gallery-timeline"]@0,120'
bun run app:control wheel "$control_session" .gallery-scroll 1400
bun run app:control wheel "$control_session" .gallery-scroll -8 ctrl
bun run app:control select "$control_session" '#settings-language-select' value=pl
bun run app:control resize "$control_session" 700 800
bun run app:control alert "$control_session" text
bun run app:control alert "$control_session" accept
```

- `wait <selector|idle> [displayed|gone|enabled|visible|hidden] [timeoutMs]` polls every
  250 ms (up to 90 s) and drains the console meanwhile. `visible`/`hidden` use computed
  visibility and opacity, which matters for the tile loader: it is always in the DOM and only
  becomes visible after 300 ms. `idle` waits for no visible tile/section loaders, no
  `aria-busy="true"`, and no paused gallery or dragged timeline.
- `modclick` holds `Shift`, `Control`, or `Control+Shift` and always releases them.
- `drag <from> <to|dx,dy|selector@dx,dy> [holdMs] [ctrl]` centers the source, presses, holds
  (default 200 ms, at least 150 ms), moves 6 px, then finishes over about 300 ms. That
  satisfies the rectangle-selection threshold and also drives the timeline scrubber and the
  windowed position bar. `ctrl` holds Control for an additive rectangle.
- `wheel <selector> <dy> [ctrl]` scrolls over the element's center. If WebKit does not report
  `ctrlKey` for the WebDriver wheel, use `native wheel ... ctrl` for the tile-size gesture.
- `resize` sets the WebView window size. Xvfb has no window manager, so maximize, minimize,
  and minimum-size behavior cannot be proven here.

Use `refresh` for a controlled reload. It records the console gap and reattaches collection.
No arbitrary JavaScript or IPC command is exposed.

### Native input

`native` drives XTest on the session's private display through `e2e/control-native.py`, for
the GTK video controls (play, mute, volume, speed popover, fullscreen) and the native file
dialogs:

```sh
bun run app:control native "$control_session" click 720 860
bun run app:control native "$control_session" key Control+l
bun run app:control native "$control_session" key Control+a
bun run app:control native "$control_session" type /path/printed/by/doctor/media
bun run app:control native "$control_session" key Return
bun run app:control native "$control_session" wheel 600 400 -2 ctrl
```

Coordinates are screen pixels on the 1440x900 display (take a screenshot first). `key`
takes X keysyms joined with `+`; the names `Control`/`Ctrl`, `Shift`, `Alt`, `Meta`, and `Super` are accepted and mapped to `Control_L` and so on, and the raw keysym spelling works too; `type` takes printable ASCII only. The helper receives an
environment built only from the session manifest and refuses, before opening any display,
unless the display is private and matches the session, `XAUTHORITY` and `XDG_DATA_HOME`
belong to the scratch root, and the scratch `owner` matches the session ID. After every
native command the controller runs a full doctor; if that fails, the session becomes
`tainted` and accepts only `logs`, `screenshot`, and `stop`. A doctor that reports
`busy-maintenance` or `unresponsive` instead does not taint the session.

In a native file picker, press `Control+l`, then `Control+a` (select all; without it the typed text only
replaces the selected file-name stem and a save becomes `tags.csv.csv`), and type exactly the
session media root (`doctor` prints it as the scan root). A CSV export must be saved outside
any indexed root (the app refuses a path inside the media root): save under `<scratch>/export`
(the controller creates it) as `<scratch>/export/tags.csv`. Never navigate to another folder.

If a path typed into a save dialog names a folder that does not exist, GTK refuses it with its
own modal "The folder contents could not be displayed" dialog, and the app never receives the
path. That dialog blocks the app's main thread, so `doctor` reports `unresponsive`. Take a
screenshot, dismiss it with `native key Escape` (doctor is `ok` again), then cancel the file
chooser with another `native key Escape` or correct the path:

```sh
bun run app:control screenshot "$control_session"
bun run app:control native "$control_session" key Escape
bun run app:control native "$control_session" key Escape
```

### Session states

`doctor` reports `state`:

- `ok`: identity, profile, roots, and processes verified.
- `alert-open`: a `window.alert` is showing (the lightbox shows one when a deleted file is
  missing or its cleanup is pending). Only `alert`, `screenshot`, `doctor`, `logs`, and
  `stop` run; read it with `alert text`, then `alert accept`.
- `busy-maintenance`: the identity is verified, but the library probe outlasted the 30 s
  script timeout. The maintenance gate used by backup export/import, and the scan/thumbnail
  locks used by Clear library and Remove root, wait without a limit. Observation commands
  (`inspect`, `wait`, `screenshot`, `logs`) still run; actions are refused. Wait with `logs`
  and `process.log` until doctor says `ok`.
- `unresponsive`: an identity probe (window title, app-data path) timed out, because the app's
  main thread does not answer, typically under a native modal dialog (see native input above).
  Observation commands, `native key Escape`, and `stop` run; other actions are refused. The
  result names the `probe` and gives a `hint`.
- `tainted`: see native input above.

While a probe hangs, each command that runs the doctor takes about 30 s before it answers, up
to about 60 s when it also waits behind a console drain.

## Evidence

```sh
bun run app:control screenshot "$control_session"
bun run app:control logs "$control_session"
bun run app:control diagnose "$control_session"
bun run app:control logs "$control_session"
```

Open the returned PNG with the Read tool (it renders images). Screenshots capture only the private
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
temporary directory. It leaves screenshots and logs intact. Repeated stop is safe. Stop does
not wait behind other commands: it ends a session whose WebView hangs within seconds, and
commands still queued at that point fail with "The session is stopping or stopped". SIGINT,
SIGTERM, and SIGHUP also clean up. Verify `session.json` says `stopped`, its temporary root
is gone, and evidence still exists. If a process will not stop, preserve the temporary data.

SIGKILL or a host crash cannot run cleanup. A leftover `active.lock` requires manual
inspection of its recorded controller and the manifest's process groups before recovery.
Never kill by name or delete a lock just because a client cannot connect. After proving
the recorded processes are gone, the exported `removeOwnedScratch(root, id)` helper in
`e2e/control-support.js` checks the ownership marker before removal.

## Helpers

The executable helper is `e2e/control.js`, exposed as `bun run app:control`; `help` lists commands.
Native input goes through `e2e/control-native.py`; `e2e/native-input.py` belongs to the Mocha
specs and is not compatible with controller sessions.

The controller creates its scratch root under `realpath(os.tmpdir())`, so `TMPDIR` decides
where it lives. `start` refuses a scratch root whose `control.sock` path would exceed the
107-byte Linux limit. Cleanup accepts only a scratch root whose parent equals the stopping
controller's `realpath(os.tmpdir())`; the controller process itself performs `stop`, so keep
`TMPDIR` unchanged for its whole life.
Run `bun run test:app-control` for socket, cleanup, process-tree, input validation, modifier
release, drag thresholds, native-helper refusals, socket-length, session-state gating,
probe-timeout classification, timeout budgets, command-queue (stop outside the queue), and
fixture checks.
Keep feature recipes current when selectors or behavior change.
