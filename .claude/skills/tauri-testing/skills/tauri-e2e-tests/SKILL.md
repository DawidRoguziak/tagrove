---
name: tauri-e2e-tests
description: Write, run and debug Tagrove's real desktop E2E - WebdriverIO 9 + tauri-driver + WebKitWebDriver against the debug .e2e build (e2e/wdio.conf.js, e2e/build-e2e.js, e2e/specs/*.e2e.js), private Xvfb/dbus/XDG runs, temp-root fixtures (e2e/fixtures.js), opt-in MEDIATAGGER_<FLAG> specs, selectors, destructive-flow safety and flaky-run triage. Use for anything under e2e/ or when a change needs the real Tauri bridge proven.
---

# Desktop E2E in Tagrove

Read `tauri-test-strategy` first: E2E is the top layer, for the real bridge, bootstrap and a
few critical or destructive journeys. Owning docs: `docs/development/testing.md` → "Real desktop
E2E" (prerequisites, build order, isolation guards, opt-in table, troubleshooting) and
`.claude/rules/testing-e2e.md`. For hands-on clicking and screenshots use `/verify-mediatagger`.

The harness is fixed: WebdriverIO + `tauri-driver` (`--native-driver` WebKitWebDriver) on
`127.0.0.1:4444`, `maxInstances: 1`, specs in JavaScript. Do **not** introduce Playwright,
`@wdio/tauri-service`, `tauri-plugin-wdio-*`, an embedded WebDriver, a browser/mocked-IPC
mode, TypeScript configs, or a CI workflow for it. Those are dependency/architecture decisions
(ADR) the project has not made.

## 1. Safety rules (never weaken)

- Identity guards: identifier `com.example.mediatagger.e2e`, title `Tagrove E2E`, target dir
  `src-tauri/target-e2e`, executable `src-tauri/target-e2e/debug/media_tagger`. They live in
  `e2e/build-e2e.js`, `e2e/wdio.conf.js` (cleans only `<XDG>/com.example.mediatagger.e2e`) and
  `e2e/control.js`. A refusal is a safety check, not a bug to work around.
- **One run at a time per checkout**: `test:e2e:tauri`, `tauri:build:e2e` and `app:control`
  share `dist/`, `target-e2e` and ports 4444/4446/4447. Don't run two; don't kill processes by
  name; don't delete a lock because a client cannot connect.
- Destructive specs (delete, clear library, remove root, restore, duplicates) index only roots
  created with `fs.mkdtemp`, and `afterEach` removes only those roots and artifacts. Never a
  personal folder, the repo root, or dev/release app data.
- Logs and screenshots go to the ignored `artifacts/`; they must not contain private paths/media.

## 2. Running on a private display

```bash
smoke_root=$(mktemp -d /tmp/mediatagger-e2e-XXXXXX)
export XDG_DATA_HOME="$smoke_root/data" XDG_CONFIG_HOME="$smoke_root/config"
export XDG_CACHE_HOME="$smoke_root/cache" XDG_RUNTIME_DIR="$smoke_root/runtime"
mkdir -p "$XDG_DATA_HOME" "$XDG_CONFIG_HOME" "$XDG_CACHE_HOME" "$XDG_RUNTIME_DIR"
chmod 700 "$XDG_RUNTIME_DIR"
unset WAYLAND_DISPLAY; export GDK_BACKEND=x11 LANG=C.UTF-8 LC_ALL=C.UTF-8 GIO_USE_VFS=local GTK_USE_PORTAL=0
dbus-run-session -- xvfb-run -a -s '-screen 0 1440x1100x24 -nolisten tcp' \
  sh -c 'export MEDIATAGGER_NATIVE_DISPLAY="$DISPLAY"; exec bun run test:e2e:tauri --spec e2e/specs/<spec>.e2e.js'
# one case: append  --mochaOpts.grep '<case>'
```

`bun run test:e2e:tauri` builds `dist/` and the debug `.e2e` binary itself. Afterwards unmount
any FUSE mounts under `$XDG_RUNTIME_DIR/doc` and `/gvfs` in the temp root, then remove only that
root. Many specs skip unless `MEDIATAGGER_<FLAG>=1`; check the opt-in table in testing.md.

## 3. Writing a spec

- Start from the closest existing spec in `e2e/specs/` (`app.smoke.e2e.js` for navigation and
  bulk actions, `app.workflows.e2e.js` for destructive/media flows, the `lightbox-*`,
  `gallery-*`, `settings-*` specs for their areas) and reuse its helpers.
- Fixtures: `e2e/fixtures.js` (`copyPngFixtures`, `createPlayableFixtures` via ffmpeg,
  `createLightboxFixtures`) into a fresh `fs.mkdtemp` root; backend assertions through
  `e2e/assets.js` (`listAssetsForAssertions`) or the spec's `invokeTauriCommand` helper.
- The harness checks the live title, then sets `media-tagger.language=en` and
  `media-tagger.theme=dark` and reloads before each spec. Translated scenarios switch language
  explicitly; persistence checks may reload inside the spec.
- Selectors: stable IDs/data attributes or accessible role/name first; English text selectors
  only when the text is the behavior. Changing copy or aria labels means updating selectors and
  the `verify-mediatagger` recipes in the same change.
- Wait on conditions (`waitForDisplayed`, `waitUntil`), never fixed pauses. WebKit
  `:focus-visible` needs native focus: use the XTest helper `e2e/native-input.py` as
  `about.e2e.js` or `tag-editor.e2e.js` do (it only runs on the private display and a
  `/tmp/mediatagger-*` `XDG_DATA_HOME`).
- Native video: WebDriver sees the session container and backend snapshot attributes, not GTK
  controls or decoded pixels. Assert on those attributes.
- New opt-in spec: gate on `process.env.MEDIATAGGER_<FLAG> === "1"` and add it to the opt-in
  table in `docs/development/testing.md`.
- Harness changes: `bun run test:e2e-build` and `bun run test:app-control` cover the build and
  control tooling.

## 4. Flaky or failing runs

Triage before retrying or raising timeouts:

1. Acting before the app is ready (wait for the gallery/settings element, not a sleep).
2. State leaking between specs (temp roots or library not cleared in `afterEach`, language/theme
   left changed).
3. Environment: port 4444 in use, missing WebKitWebDriver or `tauri-driver`
   (`WEBKIT_WEBDRIVER_PATH`, `TAURI_DRIVER_PATH`), Wayland not unset, `EPERM` on cleanup
   (stale isolated data). See testing.md → "Troubleshooting".
4. Stale frontend: the run must reach a successful `bun run build`.

Report which spec ran, on what display setup, and what was not run.
