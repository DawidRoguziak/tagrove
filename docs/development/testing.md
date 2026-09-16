# Testing

Implementation entry points: [Vitest configuration](../../vitest.config.ts), [frontend test setup](../../src/test/setup.ts), [CI workflow](../../.github/workflows/privacy.yml), [desktop harness](../../e2e/wdio.conf.js).

MediaTagger has four test layers. Use the lowest layer that can prove the behavior, then add a higher layer when the change crosses a contract or runtime boundary.

## Test layer matrix

| Layer | Location | Runner and command | What it exercises | What it does not prove |
| --- | --- | --- | --- | --- |
| Frontend unit and component | `src/**/__tests__/*.test.ts`, `src/**/__tests__/*.test.tsx`, and `src/__tests__/` | Vitest with `bun run test` | TypeScript services, hooks, React rendering and interaction, frontend state transitions, and the JavaScript shape of mocked Tauri calls | A live Tauri bridge, SQLite, native dialogs, a WebView, or real filesystem behavior |
| Rust unit | `#[cfg(test)]` modules beside backend code in `src-tauri/src/` | Cargo through `bun run test:backend` | Isolated database helpers, validation, scanning/indexing helpers, thumbnail scheduling and parsing, backup helpers, and command/service logic | A JavaScript caller, desktop window, packaged resources, or a complete application workflow |
| Rust integration and backend workflow | `src-tauri/tests/backend_integration.rs` and `src-tauri/tests/backend_e2e.rs` | Cargo through `bun run test:backend`, or the focused package scripts below | File-backed SQLite and temporary-filesystem behavior across multiple backend modules | Tauri startup, IPC serialization, WebDriver, UI, profile isolation, or packaging |
| Desktop E2E | `e2e/specs/*.e2e.js` | WebdriverIO, `tauri-driver`, and WebKitWebDriver on Linux | A real unbundled debug Tauri window, the JavaScript-to-Rust invoke bridge, selected UI workflows, and E2E profile/build isolation | Production app data, all native dialogs, or all locales |

Despite its filename, `src-tauri/tests/backend_e2e.rs` is not a desktop E2E test. It is one Rust test binary that builds a temporary file-backed database and exercises a CSV-style tag merge, media-group update, query, and library-clear workflow through Rust helpers. It does not launch Tauri, load the frontend, call the command layer through IPC, or validate the E2E Tauri profile. Only `e2e/specs/*.e2e.js` drives the desktop application.

## Package scripts

Run commands from the repository root.

| Script | Exact behavior |
| --- | --- |
| `bun run test` | Runs Vitest once through Node with an 8192 MiB old-space limit. Vitest discovers the frontend `*.test.ts` and `*.test.tsx` files. |
| `bun run test:watch` | Starts Vitest in watch mode. |
| `bun run test:backend` | Runs locked Cargo tests, including Rust library unit tests and both integration test binaries in `src-tauri/tests/`. The application binary itself has `test = false`. |
| `bun run test:backend:integration` | Runs only the locked `backend_integration` Rust integration-test binary. |
| `bun run test:backend:e2e` | Runs only the locked Rust-only `backend_e2e` workflow test binary. It is not the WebdriverIO suite. |
| `bun run test:e2e:tauri` | Runs all `e2e/specs/**/*.e2e.js` specs through `e2e/wdio.conf.js`; preparation builds the frontend and an isolated Tauri executable before starting the driver. |
| `bun run app:control` | Keeps an isolated desktop E2E session open on a private Xvfb display for UI interaction, screenshots, and console inspection; see persistent desktop control below. |
| `bun run test:app-control` | Runs Node tests for controller input validation, local-port conflicts, temporary-directory ownership, process-tree cleanup, and shared PNG/GIF/MP4 fixtures. Checks both MP4s with ffprobe for video and zero audio streams. Requires Linux local sockets, ffmpeg, and ffprobe. |
| `bun run test:locale-tools` | Runs the offline locale-validator and safe-generator regression tests through Node's built-in test runner. |
| `bun run typecheck` | Runs no-emit checks for application code and Vite/Vitest configuration. |
| `bun run lint` | Runs Biome lint across its configured source, E2E, and script includes. |
| `bun run format:check` | Checks formatting in `scripts`, `vite.config.ts`, and `vitest.config.ts`; it does not format application source or Markdown. |
| `bun run locale:check` | Checks language resources and placeholder parity offline; see [localization](localization.md#automated-locale-contract). |
| `bun run rust:fmt` | Runs `cargo fmt --all -- --check` for the backend manifest. |
| `bun run rust:clippy` | Runs locked Clippy for all targets/features, denying warnings. |
| `bun run rust:check` | Runs locked Cargo check for all targets/features. |
| `bun run audit:js` | Runs `bun audit --production --audit-level=high`. |
| `bun run audit:rust` | Runs Cargo audit against the Rust lockfile with the advisory exclusions described below. Requires `cargo-audit`. |
| `bun run quality` | Runs locale validation/tests, application and Vite/Vitest type checks, Biome lint/format checks, rustfmt, locked Clippy with warnings denied, production Bun audit, and RustSec audit with the explicit advisory exclusions in `audit:rust`. |
| `bun run test:all` | Sequentially runs `quality`, frontend Vitest, the complete locked Rust test suite, and desktop E2E. It stops at the first failed layer. |

`bun run tauri:build:e2e` is a test-support script rather than a test runner. It invokes `e2e/build-e2e.js` to create the isolated unbundled debug executable. It deliberately does not run `bun run build`, so its Tauri overlay disables `beforeBuildCommand` and it consumes the `dist/` already on disk.

Focused examples:

```bash
bun run test -- src/components/lightbox/__tests__/LightboxModal.test.tsx
bun run test -- --maxWorkers=1
cargo test --manifest-path src-tauri/Cargo.toml module_or_test_name
cargo test --manifest-path src-tauri/Cargo.toml --test backend_integration test_name
```

There is no numeric coverage threshold. Run the frontend and backend suites explicitly; they are not CI merge gates.

## Continuous integration

`.github/workflows/privacy.yml` is the only GitHub Actions workflow. It runs the `privacy` job on pull requests and pushes to `main`, using `ubuntu-24.04`. It fetches full history, installs checksum-pinned Gitleaks 8.30.1, scans publication data, and runs the privacy regression tests. It does not upload artifacts or use an Actions cache.

Frontend checks, Rust checks, application tests, and Linux packaging are run explicitly with the commands documented here and in [Linux packaging](linux-packaging.md). The local `audit:rust` script excludes `RUSTSEC-2026-0194` and `RUSTSEC-2026-0195` without recording a rationale; an excluded advisory is not proof of safety. Real desktop E2E uses the isolated `.e2e` profile on a configured Linux host. The Docker release build does not run tests.

## Frontend tests and Vitest

`vitest.config.ts` is part of the test contract:

- `environment: "jsdom"` provides a browser-like DOM, but it is not WebKitGTK or a Tauri runtime.
- `setupFiles: "./src/test/setup.ts"` loads Testing Library's jest-dom matchers, stubs `IntersectionObserver` and `ResizeObserver`, supplies `requestAnimationFrame`/`cancelAnimationFrame` fallbacks, and changes i18next to English.
- `globals: false` means every test imports `describe`, `it`, `expect`, `vi`, and other Vitest APIs explicitly.
- `css: true` makes Vitest process imported CSS instead of replacing every stylesheet with an empty stub.
- `pool: "forks"` runs tests in child-process workers. `maxWorkers: 2` limits concurrency because the suite is memory-heavy.
- `execArgv: ["--max-old-space-size=8192"]` gives each fork the configured Node heap limit. The `test` package script also gives the parent Vitest process the same limit.

Keep frontend tests next to the owning module in `__tests__`, using the existing `.test.ts`/`.test.tsx` naming. Pure services should use direct inputs and outputs. Hooks should use `renderHook`; components should use Testing Library queries and `userEvent` where user behavior matters. Mock Tauri APIs and repository API modules explicitly with `vi.mock`, reset mock implementations and timers in `beforeEach`/`afterEach`, and clear storage when a test changes persistent browser state. Do not treat a mocked `invoke` assertion as proof that Rust accepts the payload; that requires a Rust contract test or desktop E2E.

The global setup changes i18n to English asynchronously with `void i18n.changeLanguage("en")`. Tests that change language or rely on persisted language must restore their state; the setup is not rerun before every individual test.

## Rust unit and integration tests

Rust unit tests live in `#[cfg(test)]` modules beside their implementation. This is the right layer for private helpers, parsing and normalization, database query semantics, validation, scheduler behavior, and service/command branches that do not require a running Tauri application.

The integration binaries use the public `media_tagger` library API:

- `backend_integration.rs` covers file-backed asset/tag queries, root-prefix deletion and distinct thumbnail paths, complete library clearing, bulk tag merge, and bulk media-group replacement.
- `backend_e2e.rs` composes database and tag helpers into one backend workflow: export-shaped rows, CSV-style tag parsing/normalization/merge, filtering, group replacement, and library clearing. It does not invoke the CSV command parser itself.

Fixture and isolation patterns:

- Use the `tempfile` dev dependency and hold the returned `TempDir` for the full test. Its directory is removed on drop.
- Use `Connection::open_in_memory()` for database-only unit tests. Use `tempdir()/media.db`, `db::open_connection`, and `db::init_schema` when the file-backed behavior is part of the assertion.
- Create scan roots and media placeholders only below the test temp directory. Many backend tests need path identity and metadata, not decodable media, so small byte strings are intentional fixtures.
- When image decoding is the behavior under test, generate a valid image in the temp directory, as the scan-service tests do. Do not reuse a personal media library.
- Build ZIP/backup fixtures inside the temp directory and keep all restore destinations there. For filesystem-destructive code, assert both the database result and the surviving or removed files.
- Scheduler tests inject a processor and use synthetic paths; parser tests use literal tool output. They should not depend on a locally installed `ffmpeg` unless the test explicitly targets the executable boundary.

Cargo may execute independent tests concurrently. Never use a shared fixed database, app-data directory, scan root, output filename, or mutable process-global fixture. If a test truly changes process-wide state, serialize it explicitly or redesign it around injected state.

The Rust player regression generates a temporary MP4 with ffmpeg and drives the actual libmpv worker using null audio/video output. It verifies seeking, looping, replacement, settings retention, corrupt-file errors, and retry recovery without claiming GTK or OpenGL coverage. A separate test supersedes a committed session before `loadfile` and checks retirement without cancelling the replacement request.

The same worker regression runs 30 cycles of all six speed choices, alternating same-file reopening and two-file replacement. It checks autoplay from the beginning, advancing time, retained rate and rejection of stale controls, close and failure events. Real-libmpv flag tests cover true/false values, property errors and guarded C-integer storage. Run these with `cargo test --manifest-path src-tauri/Cargo.toml video_ --lib`. The frontend lifecycle checks are `useNativeVideoSession.test.tsx`, `LightboxModal.test.tsx` and `useLightboxKeyboardShortcuts.test.ts`.

## Packaging checks

`bun run test:packaging` verifies lockfile source generation, publication identity rejection, checksum inventory, duplicate-bundle rejection, and failed-build preservation with disposable artifacts. Docker builds perform format-specific artifact checks. See [package verification](linux-packaging.md) for installed-package tests and remaining desktop coverage.

## Real desktop E2E

Release production is documented in [setup and builds](setup-and-build.md#docker-packages). Its build does not run test suites.

### Linux prerequisites

1. Install the repository dependencies, normal Tauri build prerequisites, libmpv/GTK/OpenGL stack, ffmpeg, and ffprobe.
2. Install `tauri-driver`:

   ```bash
   cargo install tauri-driver --locked
   ```

3. Install WebKitWebDriver, or set `WEBKIT_WEBDRIVER_PATH` to its full path.
4. If `tauri-driver` is not at `~/.cargo/bin/tauri-driver`, set `TAURI_DRIVER_PATH` to its full path.
5. Put ffmpeg on `PATH` or set `FFMPEG_PATH` so the mixed-media workflow can generate temporary GIF and MP4 fixtures. The MP4s have no audio tracks. For `test:app-control`, put ffprobe on `PATH` or set `FFPROBE_PATH`.
6. Run under a graphical session with working GTK/OpenGL, and ensure TCP port `127.0.0.1:4444` is free.

The harness runs `media_tagger` with WebKitWebDriver. When running the suite through Xvfb from a Wayland session, unset `WAYLAND_DISPLAY` and set `GDK_BACKEND=x11`; setting `DISPLAY` alone does not force GTK onto the private display. Only the E2E window profile permits resizing down to 320 × 240 so responsive lightbox behavior can be tested; dev and release retain their 1000 × 720 minimum.

### Build, driver, and session order

`bun run test:e2e:tauri` performs these steps in order:

1. WebdriverIO `onPrepare` records the start time and validates the E2E build configuration.
2. It removes only the expected old E2E executable and calls the guarded E2E app-data cleanup.
3. It runs `bun run build`, producing a fresh frontend `dist/`.
4. `buildE2eApp()` validates again, removes only the expected executable, sets `CARGO_TARGET_DIR` to the isolated target, and runs `tauri build --debug --no-bundle --config src-tauri/tauri.conf.e2e.json -- --locked`.
5. The build helper verifies that the expected executable exists.
6. WebdriverIO `beforeSession` starts `tauri-driver` with `--native-driver <WEBKIT_WEBDRIVER_PATH>`.
7. WebdriverIO opens one session for the isolated application. `maxInstances: 1` keeps the shared desktop state serial.
8. Before tests run, the suite reads the live window title and refuses to continue unless it exactly matches the E2E title.
9. `afterSession` and process signal handlers stop `tauri-driver`.

The exact E2E identity is:

| Property | Required value |
| --- | --- |
| Tauri identifier | `com.example.mediatagger.e2e` |
| Product/window title | `Tagrove E2E` |
| Cargo target directory | `src-tauri/target-e2e` |
| Executable | `src-tauri/target-e2e/debug/media_tagger` |
| Build kind | Debug, unbundled (`--debug --no-bundle`) |
| WebDriver endpoint | `127.0.0.1:4444` |
| Production identifier kept separate | `com.example.mediatagger` |

Tauri derives the E2E app-data directory from `${XDG_DATA_HOME:-~/.local/share}/com.example.mediatagger.e2e`.

### Isolation guarantees and cleanup guards

The build helper refuses to build unless all of these conditions hold:

- the overlay identifier is exactly `com.example.mediatagger.e2e` and ends in `.e2e`;
- the configured window list contains the exact title `Tagrove E2E`;
- the resolved target directory is not `src-tauri/target` and ends in `target-e2e`.

The pre-run app-data cleanup resolves both paths and refuses recursive deletion unless:

- the E2E and production app-data paths differ;
- the E2E directory's parent is exactly the resolved app-data root;
- its basename is exactly `com.example.mediatagger.e2e`;
- the identifier ends in `.e2e`.

Only then does it recursively remove the E2E directory with retries. An `EPERM` lock is a limitation: the runner warns, skips that cleanup, and continues, so stale isolated E2E state may remain. The live-title check is the final guard before specs containing destructive actions.

### Fixtures, selectors, and destructive workflows

`app.smoke.e2e.js` indexes the checked-in PNG assets under `src-tauri/icons` and exercises navigation, settings, bulk tags, and bulk groups. It does not delete those source files. `app.workflows.e2e.js` creates unique roots with `fs.mkdtemp`, uses `e2e/fixtures.js` to copy valid PNG icons or use ffmpeg to generate a small GIF and two MP4 files without audio tracks, and records temporary CSV/ZIP artifact paths under the operating-system temp directory. Its `afterEach` clears the isolated library database and removes only those roots and artifacts. Video coverage includes picture clicks to pause/resume and focused Left/Right seeking by five seconds, plus native IPC checks for paused seeking, fullscreen session continuity, retained preferences, autoplay on replacement, and stale closes. These checks use backend snapshot attributes, not GTK button clicks or video pixels. Other video coverage checks authorized native opening, the full native-player footprint, absence of a DOM control rail, rapid source switching, absence of an HTML `<video>`, error fallback, and native-surface cleanup. WebDriver cannot inspect pixels or controls rendered by GTK above the WebView.

The workflow suite deliberately tests library clear, scan-root removal, backup restore, and permanent media deletion. Keep every deletable media fixture under a newly created temp root. Never change a destructive spec to index a personal directory, the repository root, or production app data. Do not weaken the identifier, target-directory, app-data-path, or live-title guards to make a failing run proceed.

Desktop selectors currently mix stable IDs/classes/data attributes, `aria-label` values, visible English text selectors such as `button=Apply`, and a small number of XPath expressions. Prefer stable accessible roles/names or explicit test IDs when text is not itself the behavior. If visible copy or accessibility labels change, update the associated E2E selectors in the same change.

The desktop harness does not force English. A fresh app chooses a stored language first and otherwise follows `navigator.language`, while the current specs assert English labels. Because pre-run cleanup removes the isolated profile, a manually persisted selection is not a durable fix; run with a browser environment that reports English or update the harness so language setup is deterministic. Vitest does force English through `src/test/setup.ts`; that does not affect the desktop process.

### UI redesign matrix

`e2e/specs/ui.redesign.e2e.js` is opt-in with `MEDIATAGGER_UI_REDESIGN=1`. It uses fresh
copied media and the normal isolated E2E identity. It exercises both themes at 1440×900 and
1000×720, search/suggestions, validation and empty results, bulk selection, tag browsing,
lightbox deletion cancellation, settings navigation, duplicates and confirmation dialogs.
It verifies theme persistence after reload, native arrow-key changes in the theme and media
radio groups, and a visible focus ring when tabbing to the selected media segment.
Contrast checks read the live theme tokens and require 4.5:1 for main/primary text on all
three surfaces and secondary text on workspace/panels. Inactive segments use main text
because the light secondary color does not meet 4.5:1 on raised controls.
The fixtures include long filenames and tags.
It also checks Polish gallery/settings/lightbox text, both-theme drawers at 600px and 320px, the stacked bulk layout at 900px, native-control appearance, and failure to decode a temporary original. Screenshots capture
the private X11 display into a fresh timestamped `artifacts/ui-redesign/` subdirectory per run.
The normal workflow suite covers narrow lightbox and native media behavior; the separate
`MEDIATAGGER_GALLERY_PERF=1` suite exercises 2,048 images and cache eviction. It also checks selection summary IPC and full-selection metadata writes after eviction and filter changes. See [frontend refactor verification](frontend-refactor-verification.md) for the measured run and retention bounds.

Run the opt-in spec through the existing harness on a private X11 display with temporary
XDG data/config/cache directories. Preserve all profile and cleanup guards. Example after
setting those XDG directories and starting a private display:

```sh
MEDIATAGGER_UI_REDESIGN=1 bun run test:e2e:tauri --spec e2e/specs/ui.redesign.e2e.js
```

### Animation geometry

`e2e/specs/animations.e2e.js` is opt-in with `MEDIATAGGER_ANIMATIONS=1`. It seeds
192 temporary PNGs and long labels, then samples scroll dimensions and positions on
every rendering frame before, during and after interaction. Both themes run at 699,
700, 767, 768, 999 and 1000 CSS pixels. Hover/press, bottom-of-gallery selection,
suggestions, standard dialogs, bulk inspector, inline information/confirmation and
repeated lightbox drawer toggles are covered. Modal samples also record opening and
closing opacity, durations, accessibility hiding, inertness and fixed shell bounds.
The `modal media lifecycle` case checks image/GIF/video opening, navigation without
another entrance, closing and fullscreen, saving synthetic screenshots. Run just
that case with `--mochaOpts.grep 'modal media lifecycle'`. Native fullscreen video
uses Escape to return to the window before Close preview; its immediate playback
shutdown during the exit is covered separately by the lightbox component tests. Layout-changing actions are paired with
the same actions with decorative CSS animation and transitions disabled and the
modal duration tokens set to zero. The existing
clipped drawer slide stays enabled in both halves of the comparison. Filtering, resizing and
intentional navigation are not required to preserve scroll position.
The segmented-control cases additionally sample every label and adjacent toolbar child
through mouse selection, native arrow keys and rapid changes. They check intermediate
highlight movement, final alignment, keyboard focus and immediate resize placement in
both themes, English and Polish, at 320, 699 and 1000 CSS pixels. A query with no matches
holds result counts constant so functional count changes do not affect this comparison.
Use `--mochaOpts.grep 'segmented selection'` to run only these cases. The same cases assert
immediate highlight placement when running with the real reduced-motion preference.

Worker-pending suggestion shells are hidden and excluded from popup comparisons.
A metadata loading row can appear for a single frame in only one run; its temporary
content height is excluded from the range comparison. Scroll positions, viewport
dimensions and the complete settled layout remain checked.

Run twice on a private X11 display with temporary XDG directories, as for the redesign
matrix. Set `gtk-enable-animations=true` in the temporary
`$XDG_CONFIG_HOME/gtk-3.0/settings.ini` for the first run, then `false` for the second.
Each file needs a `[Settings]` header. Restart the desktop app between runs so WebKit
reads the GTK preference. Use a private D-Bus session and a temporary GSettings keyfile
backend as well; the GTK file alone did not activate the preference on the verified host.
With the temporary XDG directories already exported, run:

```sh
export GSETTINGS_BACKEND=keyfile
export GDK_BACKEND=x11
gsettings set org.gnome.desktop.interface enable-animations false
MEDIATAGGER_ANIMATIONS=1 MEDIATAGGER_REDUCED_MOTION=1 dbus-run-session -- xvfb-run -a bun run test:e2e:tauri --spec e2e/specs/animations.e2e.js
```

Set the key and GTK file to `true` and omit `MEDIATAGGER_REDUCED_MOTION` for normal motion.
Force `GDK_BACKEND=x11` so GTK uses the private Xvfb display even on a Wayland host.
The GTK WebView must have native focus for WebKit to expose `:focus-visible`. On bare
Xvfb, activate it with a native click in the window; WebDriver DOM clicks alone do not
activate the GTK widget. The segmented cases assert `document.hasFocus()` before sampling.
Keep these settings inside the temporary profile. WebKitGTK 2.52 reads
[`gtk-enable-animations`](https://github.com/WebKit/WebKit/blob/webkitgtk-2.52.6/Source/WebKit/UIProcess/gtk/SystemSettingsManagerProxyGtk.cpp).
The spec asserts the real `prefers-reduced-motion` query;
injected CSS supplies only the comparison baseline, never preference emulation.

Frame samples and failure screenshots stay in a timestamped `artifacts/animations/`
directory. The component tests cover focus trapping, inertness, Escape and restoration;
they cannot prove desktop scroll geometry.

The `ui.redesign.e2e.js` case selected by `--mochaOpts.grep 'corner badges'` checks the Video/GIF badge colors and offsets, floating image/GIF controls down to 320px, and modal-only bulk ordering in both themes. Use `MEDIATAGGER_UI_REDESIGN=1` and the same isolated desktop setup.

### Gallery thumbnail fade

`e2e/specs/thumbnail-fade.e2e.js` is opt-in with `MEDIATAGGER_THUMBNAIL_FADE=1`.
It creates 192 temporary colorful PNGs and samples image opacity on rendering frames
during rapid tag-filter changes and native wheel scrolling in both themes. It checks
the 300 ms ease-in-out fade, full opacity on tile backgrounds and the gallery container,
and unchanged loaded images staying visible. Screenshots and samples remain under
`artifacts/thumbnail-fade/`.

Run on a private X11 display with temporary XDG directories, using the GTK and GSettings
setup in [animation geometry](#animation-geometry). Run again with animations disabled
and `MEDIATAGGER_REDUCED_MOTION=1` to check the real reduced-motion preference.

```sh
MEDIATAGGER_THUMBNAIL_FADE=1 bun run test:e2e:tauri --spec e2e/specs/thumbnail-fade.e2e.js
```

`ThumbnailImage.test.tsx` covers delayed/cached/failed loads, transparent placeholders,
source replacement, unchanged-source rerenders, and the default without fading.

### Gallery selection gestures

`e2e/specs/gallery-selection.e2e.js` is opt-in with `MEDIATAGGER_GALLERY_SELECTION=1`. It uses native pointer actions for additive clicks, Ctrl-click toggling, replacing and additive rectangles, shrinking, post-drag click suppression, and edge scrolling in both themes. It also checks Escape clearing and drag cancellation, unused space below a short gallery, and control clicks. A 2,051-file temporary collection proves selection across unloaded pages and eviction; normal-mode clicks still open the lightbox. Screenshots are saved under `artifacts/gallery-selection/`.

Run through the existing isolated harness on a private X11 display with temporary XDG directories:

```sh
MEDIATAGGER_GALLERY_SELECTION=1 bun run test:e2e:tauri --spec e2e/specs/gallery-selection.e2e.js
```

### Native speed menu and repeated playback

`e2e/specs/video-speed.e2e.js` is opt-in with `MEDIATAGGER_NATIVE_VIDEO=1`. It uses generated MP4s and native XTest pointer/key events through `e2e/native-input.py`. WebDriver reads session snapshots and supplies DOM geometry; it does not click GTK controls. The spec checks outside dismissal over the picture, controls, sidebar and backdrop, trigger toggling, Escape before fullscreen, focus loss, zero bounds, stale bounds, and 30 speed-change/close/reopen cycles across two videos. Some cycles explicitly close the backend session while the menu is open to check teardown.

The bottom-button fullscreen regression runs at 1000px and 600px in both themes with WebView, native play-button, and speed-menu focus. It pauses the short looping fixture to measure position preservation, checks the session and sidebar choice, and clicks the floating reopen/close controls above native video. Run just this case with `--mochaOpts.grep 'bottom-button fullscreen'`.

Each cycle captures two native screenshots and uses `e2e/video-frames.py` to require colored, changing pixels inside the picture, excluding controls and sidebar. Evidence and pixel measurements remain under `artifacts/video-speed/<timestamp>/`. Dependencies beyond the ordinary desktop harness are Python with Pillow and PyGObject, libXtst, and ImageMagick `import`. The focus fixture opens a temporary GTK window in the isolated session.

Use a fresh `/tmp/mediatagger-*` profile and an authenticated private Xvfb display. Set `MEDIATAGGER_NATIVE_DISPLAY` to that display only. The helper refuses ordinary `:0` and `:1` displays. For example, with temporary XDG data/config/cache directories already exported:

```sh
GDK_BACKEND=x11 MEDIATAGGER_NATIVE_VIDEO=1 dbus-run-session -- xvfb-run -a -s '-screen 0 1440x1000x24 -nolisten tcp' sh -c 'export MEDIATAGGER_NATIVE_DISPLAY="$DISPLAY"; bun run test:e2e:tauri --spec e2e/specs/video-speed.e2e.js'
```

For Wayland, use a private compositor and display its development viewer inside private Xvfb. Run the app with `GDK_BACKEND=wayland`, its private Wayland socket and isolated runtime directory. Native input targets the viewer, which forwards it through the compositor. `MEDIATAGGER_NATIVE_X` and `MEDIATAGGER_NATIVE_Y` account for viewer chrome; verify offsets against screenshots. The GTK controls are measured at scale 1 with the default theme. Record the actual `native video GDK backend` startup line; the host's session type alone does not identify the app backend. A Wayland viewer run does not prove behavior on every compositor or GPU.

### Lightbox controls and inactivity

`e2e/specs/lightbox-activity.e2e.js` is opt-in with `MEDIATAGGER_LIGHTBOX_ACTIVITY=1`.
It generates PNG, GIF and MP4 fixtures and checks both themes at 1000px and 600px.
Native clicks and keys verify header Close placement, keyboard collapse/reopen focus,
the first Enter after idle, computed opacity and pointer hit testing, and unchanged
media rectangles and native session identity through the fade. The open panel stays visible.
The group-copy and hover cases check the 32px sidebar button placement and copy feedback
at both widths. Native pointer movement over metadata and island padding keeps the bottom
box clickable beyond three seconds while top controls hide; exiting into surrounding row
space restores delayed hiding. Run just these cases with `--mochaOpts.grep 'keeps the bottom box'`.
Samples and private-display screenshots remain under `artifacts/lightbox-activity/`.

Use temporary XDG directories, a private D-Bus session, and authenticated Xvfb as in
[native speed verification](#native-speed-menu-and-repeated-playback). Set the temporary
GTK/GSettings animation preference as in [animation geometry](#animation-geometry),
then run:

```sh
GDK_BACKEND=x11 MEDIATAGGER_LIGHTBOX_ACTIVITY=1 dbus-run-session -- xvfb-run -a -s '-screen 0 1440x1000x24 -nolisten tcp' sh -c 'export MEDIATAGGER_NATIVE_DISPLAY="$DISPLAY"; bun run test:e2e:tauri --spec e2e/specs/lightbox-activity.e2e.js'
```

Repeat with the temporary animation preference disabled and `MEDIATAGGER_REDUCED_MOTION=1`.
Hook tests cover the exact three-second deadline, movement filtering, pointer presses,
wheel/scroll/keyboard resets, native activity callbacks, touch visibility and cleanup.
Component tests cover pending-deletion guards and the first navigation shortcut after idle.

### Persistent desktop control

`e2e/control.js` reuses the E2E build configuration and shared media fixtures without running
the Mocha suite. The [MediaTagger verification skill](../../.cursor/skills/verify-mediatagger/SKILL.md)
contains the complete launch, doctor, interaction, evidence, and cleanup workflow, with
recipes for gallery/search, settings, and lightbox.

```sh
bun run app:control start
```

Keep `start` running in its terminal or tool exec session. Wait for the `READY` line, then use
its session ID in separate commands. `bun run app:control help` documents the arguments.
The available commands are `doctor`, `inspect`, `click`, `fill`, `keys`, `scroll`, `screenshot`,
`logs`, `refresh`, `diagnose`, and `stop`. Interaction commands check the live E2E title,
resolved app-data path, scan roots, and owned display/driver processes before acting.

In addition to normal desktop prerequisites, this controller needs Xvfb, xauth, and
ImageMagick's `import`. It creates an authenticated private display with TCP disabled and
uses local driver ports 4446/4447 plus a private Unix socket. Sandbox environments must allow
these local sockets and native processes. An occupied port is refused; the controller does
not attach to another instance. A checkout lock refuses a second controller. Do not run
ordinary desktop E2E or another frontend/E2E build concurrently with it, because build
outputs are shared.

Each session gets fresh XDG data/config/cache directories below a new temporary root. It
builds fresh frontend assets and the existing `.e2e` executable, copies four PNGs, generates
the shared GIF/two MP4 fixtures, and seeds only that temporary media directory. The MP4s
have no audio tracks. It sets English in this isolated WebView before reloading. Existing dev/release profiles and the
ordinary E2E runner's cleanup behavior are unchanged.

Evidence is retained under ignored `artifacts/app-control/<session>/`: PNG screenshots,
`actions.jsonl`, `console.jsonl`, `process.log`, and `session.json`. `logs` returns the latest
24,000 characters from each log. Frontend console collection attaches after connection,
preserves original console behavior, and records reload gaps or buffer overflow. It does
not capture the earliest frontend startup messages or worker consoles. Driver and inherited
application stdout/stderr are recorded from launch. `diagnose` emits known test messages;
it does not prove application error handling.

Screenshots capture the private display, including GTK overlays. Gallery/image screenshots
were visually verified. The controller's original verification did not establish decoded
MP4 playback. The separate native speed regression above checks decoded picture changes
with native input. WebDriver selectors still cannot control native GTK playback buttons
or native file dialogs. Preserve isolation when checking those paths.

`stop` and handled signals close the WebDriver session and owned process groups before
removing the marked temporary root. Stop is repeatable, and evidence survives. Failed
startup uses the same cleanup. If owned processes cannot be stopped, temporary data stays
in place. SIGKILL/host crashes can leave a lock and temporary state; inspect recorded
process identities before manual recovery, never kill by process name or automatically
remove a lock merely because a connection failed.

## Privacy and source packaging regressions

`bun run test:privacy` runs `scripts/test_privacy.py` against temporary Git repositories with the real Gitleaks 8.30.1 executable on `PATH`. It verifies historical and staged secrets, private home paths, nested configuration/databases/keys, shallow-clone rejection and source archives that match a clean commit. `bun run privacy:check` scans this checkout and all reachable history; it is not a scan of ignored working materials.

`bun run test:packaging` also requires Gitleaks because failure/rollback fixtures pass through actual source preparation before their simulated Docker failures. They assert that the intended Docker/export failure was reached. For the real bundle audit and prerequisites, see [release privacy checks](linux-packaging.md#privacy-gate-and-release-inputs).

## Choosing the test level

For documentation-only changes, verify local links/anchors, referenced commands and symbols, and `git diff --check`. Run an existing focused test only when it resolves a behavior claim. Builds, release scripts, and destructive desktop workflows are not documentation checks.

| Change | Minimum focused proof | Add when applicable |
| --- | --- | --- |
| Pure TypeScript service, parser, or reducer | Vitest unit test | Component/hook test if rendering or React lifecycle matters |
| React component, hook, keyboard behavior, modal, or accessibility contract | Vitest with Testing Library | Desktop E2E for behavior dependent on WebView/Tauri or a high-value full workflow |
| Rust helper, SQL query, validation, scheduler, scan, thumbnail, or backup logic | Rust unit test | File-backed integration test for transactions or filesystem/SQLite interaction |
| Public backend workflow spanning database modules | `backend_integration` or the Rust-only `backend_e2e` binary | Desktop E2E if the JavaScript command boundary or UI is part of the contract |
| Tauri command name/payload, serialization, event/channel, startup, profile, or app-data behavior | Rust test plus frontend API test | Desktop E2E to prove the real bridge and bootstrap |
| Destructive UI confirmation, filesystem mutation, import/export/restore, or critical user journey | Lower-layer tests for edge cases | Isolated desktop E2E for the representative happy path and safety guard |
| Packaging, native libraries, or release resources | Build and lower-layer tests | Build and smoke-test the Linux artifact; release scripts do not run application tests automatically |

## Known limitations

- jsdom and mocked Tauri calls do not prove IPC serialization or native runtime behavior.
- `backend_e2e.rs` does not test the desktop, command serialization, the native CSV dialog/parser path, or profile isolation.
- The desktop suite is an unbundled debug build and does not verify the release artifact's exact shared-library environment.
- Desktop E2E requires a configured Linux host. The release container does not include WebKitWebDriver, Xvfb, or `tauri-driver`.
- Desktop selectors assume English, but the harness does not currently set the application language deterministically.
- An `EPERM` during E2E app-data deletion leaves stale isolated data and does not fail fast.
- The standalone `tauri:build:e2e` script can embed stale `dist/`; only the full desktop test command builds the frontend first.
- Native video playback depends on libmpv, GTK, and OpenGL. WebDriver sees the native session container and backend snapshot attributes, but not decoded `GtkGLArea` pixels or the native GTK control overlay.
- Backend fixtures cover many filesystem semantics but do not constitute exhaustive testing on every supported filesystem or operating system.

## Troubleshooting

| Symptom | Check and resolution |
| --- | --- |
| Cannot connect to `127.0.0.1:4444`, address already in use, or driver exits immediately | Stop stale `tauri-driver`/WebDriver processes and confirm port 4444 is free. Only one configured suite can use the endpoint at a time. |
| Session creation fails because the native driver is missing | Install WebKitWebDriver or point `WEBKIT_WEBDRIVER_PATH` at it. |
| `tauri-driver` is not found | Install it with Cargo or set `TAURI_DRIVER_PATH` to the full executable path. The default is `~/.cargo/bin/tauri-driver`. |
| E2E app data cannot be removed or old state appears | Close the E2E application and stale drivers, then rerun. An `EPERM` warning means cleanup was skipped; inspect only the isolated `com.example.mediatagger.e2e` directory, never delete production app data as a workaround. |
| Standalone E2E build shows old frontend behavior | `bun run tauri:build:e2e` does not rebuild `dist/`. Run `bun run build` first, or use `bun run test:e2e:tauri`, which does so automatically. |
| E2E build refuses its identifier, title, or target | Restore `src-tauri/tauri.conf.e2e.json` and `e2e/build-e2e.js` to the exact isolated values. The refusal is a safety check, not a cache problem. |
| Expected English heading/button is missing | Verify the isolated app's selected language. The specs use English text and `aria-label` selectors; the browser locale can choose another language on first start. |
| Vitest process is killed or runs out of memory | Run a focused file and retry with `bun run test -- --maxWorkers=1`. Keep large mocks/DOM trees cleaned up; the configured default is two forks with an 8192 MiB heap limit. |
| A frontend test cannot find `IntersectionObserver`, `ResizeObserver`, or animation-frame APIs | Confirm the test is using the repository `vitest.config.ts` and `src/test/setup.ts` rather than invoking an alternate config. |
| Cargo test leaves or collides with data | Move the fixture under `tempfile::tempdir()` or use an in-memory database. Remove shared fixed paths and process-global mutable state. |

## Change checklist

- Choose the lowest test layer that observes the changed behavior.
- Add a regression assertion for a bug before or with the fix.
- Keep frontend mocks reset and make asynchronous state transitions explicit.
- Keep Rust filesystem/database fixtures unique and temporary.
- For IPC changes, verify the Rust command contract and the frontend payload; use desktop E2E for a critical bridge path.
- For destructive desktop tests, confirm all media, artifacts, app data, and build output remain under the isolated paths above.
- Run the focused test while iterating, then the complete affected layer.
- Run `bun run test:all` when the change crosses frontend, backend, and desktop workflow boundaries and the Linux desktop prerequisites are available.
- For packaging/resource changes, supplement automated tests with an installed-package smoke test.

## Startup scan regression

`e2e/specs/startup-scan.e2e.js` creates two disposable media folders, enables one through the Settings checkbox, and uses fresh WebDriver sessions to launch new app processes. It proves selected-folder scanning, persistence, no automatic scan after toggling/Settings navigation/frontend reload, next-launch discovery, disabling on a later launch, and manual Rescan all including unchecked folders. Run it with the normal isolated desktop harness:

```sh
bun run test:e2e:tauri -- --spec ./e2e/specs/startup-scan.e2e.js
```

The focused frontend regression is `useStartupScan.test.tsx`; Rust tests cover launch-snapshot contention, schema migration, preference persistence, and old/new backup compatibility with root remapping.

## Backend runtime regressions

`backend_runtime.rs` exercises precise fingerprint/version coupling, version-2 failure migration, dedicated FULL-synchronous connections, and a real 514-file scan across batching and overlapping roots. Unit tests cover maintenance admission/draining, pool callback cleanup, query cache budget/epoch, stale thumbnail stream publication and frontend reload, directory-sync failures after successful moves, disk-staged CSV row order/clears, and bounded subprocess diagnostics/timeouts.

Run the opt-in metadata measurements alone so SQLite's process-wide allocator counters are attributable:

```sh
cargo test --manifest-path src-tauri/Cargo.toml --test backend_runtime -- --ignored --nocapture --test-threads=1
```

The fixtures contain 1,000 and 10,000 metadata rows and no media collection. The output distinguishes Rust ID-vector capacity from additional peak SQLite allocation and checks indexed filename lookup plus seven-row thumbnail pages. See [the verification record](backend-refactor-verification.md) for measured results and desktop limitations.

## Review fix regressions

`e2e/specs/review-fixes.e2e.js` uses disposable media to verify navigation after a group-order save beyond the first 128-item page, literal tag include/exclude and autocomplete, and thumbnail clearing after a changed-file rescan. It saves screenshots and the clear/regeneration result under `artifacts/app-review/2026-09-06/fix-verification/`. Run it with the isolated desktop harness:

```sh
bun run test:e2e:tauri -- --spec ./e2e/specs/review-fixes.e2e.js
```

Lower-layer regressions are `queryRefreshNavigation.test.tsx`, `literalTagSearch.test.ts`, the library hook/API tests, the query-position service test, and `src-tauri/tests/thumbnail_clear.rs`. The Rust file-backed test also checks nested obsolete files, skipped symlinks, exclusive-reader coordination, failure-marker removal, source preservation, and actual image regeneration.

## About and publisher metadata

`e2e/specs/about.e2e.js` is opt-in with `MEDIATAGGER_ABOUT=1`. On an authenticated
private Xvfb display with temporary XDG directories, run:

```sh
GDK_BACKEND=x11 MEDIATAGGER_ABOUT=1 dbus-run-session -- xvfb-run -a -s '-screen 0 1440x900x24 -nolisten tcp' sh -c 'export MEDIATAGGER_NATIVE_DISPLAY="$DISPLAY"; bun run test:e2e:tauri --spec e2e/specs/about.e2e.js'
```

Use a fresh `/tmp/mediatagger-*` data/config/cache profile, as in native video
verification. The native input helper requires libXtst and refuses the ordinary
`:0` and `:1` displays. The spec checks About navigation and focus at the bottom of
Settings, the package version and exact bundled GPL/privacy text, Polish and English,
and both themes. It exercises real Opener capability rejection for unrelated URLs,
file opening and explicit application selection. One test changes only the outgoing
Opener URL at the fetch boundary to provoke a real native denial after a button click;
it verifies the error UI and native focus and keyboard selection of the original address.
Screenshots remain in ignored `artifacts/about/` directories.

`AboutSection.test.tsx` covers both links, opener rejection/retry, metadata, local
text and navigation. Packaging tests check local/publication IDs across generated
files, case-insensitive GitHub identity, publisher/capability consistency, missing
release fields, and legal/capability/publisher inputs in a source archive. Release
fields are excluded from publisher identity comparison because an archive cannot
contain its own final checksum.

For a real browser handoff, use the persistent desktop controller with a browser
profile and MIME-handler configuration inside its disposable XDG directories. Check
both source and contact buttons, then close only that browser before stopping the
controller. The September 2026 verification opened the repository and issues page
in Firefox on the private display, without signing in or submitting anything.
This unbundled desktop check does not verify a Flatpak portal or an installed release
package. Opener reports errors returned by its launch call; a browser failure after
a successful detached launch may not be reported to the application.
