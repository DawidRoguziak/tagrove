# Testing

Implementation entry points: [Vitest configuration](../../vitest.config.ts), [frontend test setup](../../src/test/setup.ts), [CI workflow](../../.github/workflows/privacy.yml), [desktop harness](../../e2e/wdio.conf.js).

Tagrove has four test layers. Use the lowest layer that can prove the behavior, then add a higher layer when the change crosses a contract or runtime boundary.

## Test layer matrix

| Layer | Location | Runner and command | What it exercises | What it does not prove |
| --- | --- | --- | --- | --- |
| Frontend unit and component | `vitest.config.ts` includes `src/**/*.{test,spec}.{ts,tsx}`; by convention tests live in `src/**/__tests__/` | Vitest with `bun run test` | TypeScript helpers, models, Solid rendering and interaction, frontend state transitions, and the JavaScript shape of mocked Tauri calls | A live Tauri bridge, SQLite, native dialogs, a WebView, or real filesystem behavior |
| Rust unit | `#[cfg(test)]` modules beside backend code in `src-tauri/src/` | Cargo through `bun run test:backend` | Isolated database helpers, validation, scanning/indexing helpers, thumbnail scheduling and parsing, backup helpers, and command/service logic | A JavaScript caller, desktop window, packaged resources, or a complete application workflow |
| Rust integration and backend workflow | All Cargo-discovered integration targets in `src-tauri/tests/` | Cargo through `bun run test:backend`, or `cargo test --manifest-path src-tauri/Cargo.toml --locked --test <target>` for one target | File-backed SQLite and temporary-filesystem behavior across multiple backend modules; ignored diagnostics require explicit opt-in | Tauri startup, IPC serialization, WebDriver, UI, profile isolation, or packaging |
| Desktop E2E | `e2e/specs/*.e2e.js` | WebdriverIO, `tauri-driver`, and WebKitWebDriver on Linux | A real unbundled debug Tauri window, the JavaScript-to-Rust invoke bridge, selected UI workflows, and E2E profile/build isolation | Production app data, all native dialogs, or all locales |

Despite its filename, `src-tauri/tests/backend_e2e.rs` is not a desktop E2E test. It is one Rust test binary that builds a temporary file-backed database and exercises a CSV-style tag merge, media-group update, query, and library-clear workflow through Rust helpers. It does not launch Tauri, load the frontend, call the command layer through IPC, or validate the E2E Tauri profile. Only `e2e/specs/*.e2e.js` drives the desktop application.

## Package scripts

Run commands from the repository root.

| Script | Exact behavior |
| --- | --- |
| `bun run test` | Runs Vitest once through Node with an 8192 MiB old-space limit. Vitest discovers the frontend `*.test.ts` and `*.test.tsx` files. |
| `bun run test:watch` | Starts Vitest in watch mode. |
| `bun run test:build` | Runs a Vite production build and asserts the Solid entry, no React production modules/dependencies and no legacy source trees. Does not run TypeScript; use `build` or `typecheck` too. |
| `bun run test:backend` | Runs locked Cargo tests, including Rust library unit tests and all Cargo-discovered integration targets in `src-tauri/tests/`. Tests marked `#[ignore]` are skipped unless explicitly requested. The application binary itself has `test = false`. |
| `bun run test:backend:integration` | Runs only the locked `backend_integration` Rust integration-test binary. |
| `bun run test:backend:e2e` | Runs only the locked Rust-only `backend_e2e` workflow test binary. It is not the WebdriverIO suite. |
| `bun run test:e2e:tauri` | Runs all `e2e/specs/**/*.e2e.js` specs through `e2e/wdio.conf.js`; preparation builds the frontend and an isolated Tauri executable before starting the driver. |
| `bun run app:control` | Keeps an isolated desktop E2E session open on a private Xvfb display for UI interaction, screenshots, and console inspection; see persistent desktop control below. |
| `bun run test:e2e-build` | Checks frontend build order, profile rejection, preference isolation, and WDIO failure exit with fake build tools in a disposable project. |
| `bun run test:app-control` | Runs Node tests for controller input validation, local-port conflicts, temporary-directory ownership, process-tree cleanup, and shared PNG/GIF/MP4 fixtures. Checks both MP4s with ffprobe for video and zero audio streams. Requires Linux local sockets, ffmpeg, and ffprobe. |
| `bun run test:locale-tools` | Runs the offline locale-validator and safe-generator regression tests through Node's built-in test runner. |
| `bun run typecheck` | Runs separate no-emit checks for the Solid application/tests and Vite/Vitest configuration. |
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

`bun run tauri:build:e2e` is a test-support script rather than a test runner. It invokes `e2e/build-e2e.js` to create the isolated unbundled debug executable. It runs `bun run build` for the application before invoking Tauri. Its overlay disables the redundant `beforeBuildCommand`.

Focused examples:

```bash
bun run test -- src/features/lightbox/__tests__/LightboxModal.test.tsx
bun run test -- --maxWorkers=1
cargo test --manifest-path src-tauri/Cargo.toml --locked module_or_test_name
cargo test --manifest-path src-tauri/Cargo.toml --locked --test backend_integration test_name
```

There is no numeric coverage threshold. Run the frontend and backend suites explicitly; they are not CI merge gates.

Solid library model tests live beside `src/library` owners and run with `bun run test -- src/library`.
They use Solid Testing Library's owned `renderHook` and `src/test/settle.ts` for explicit
batch boundaries. Deferred promises control races; queue tests use fake timers. The model
integration suites cover mutation admission/draining, canonical publication, query and
identity invalidation, cache bounds and per-asset subscription cleanup. They mock platform
IPC and do not establish native-bridge or desktop feature parity.

The opt-in `e2e/specs/search-recovery.e2e.js` suite runs with
`MEDIATAGGER_SEARCH_RECOVERY=1`. It uses temporary PNG fixtures and faults the first
suggestion-worker construction after each reload. Retry then runs the real bundled worker.
It checks native pointer and Tab/Enter recovery in search, lightbox and bulk editors,
plus literal `constructor` and `__proto__` selection through the rendered tag modal.
Run it on a private display with disposable XDG directories, using the isolation rules below.

## Continuous integration

`.github/workflows/privacy.yml` is the only GitHub Actions workflow. It runs the `privacy` job on pull requests and pushes to `main`, using `ubuntu-24.04`. It fetches full history, installs checksum-pinned Gitleaks 8.30.1, scans publication data, and runs the privacy regression tests. It does not upload artifacts or use an Actions cache.

Frontend checks, Rust checks, application tests, and Linux packaging are run explicitly with the commands documented here and in [Linux packaging](linux-packaging.md). The local `audit:rust` script excludes `RUSTSEC-2026-0194` and `RUSTSEC-2026-0195` without recording a rationale; an excluded advisory is not proof of safety. Real desktop E2E uses the isolated `.e2e` profile on a configured Linux host. The Docker release build does not run tests.

## Frontend tests and Vitest

`vitest.config.ts` runs one Solid project across all source tests, using the Solid plugin,
browser export conditions and inlined Solid dependencies. Pure tests execute once in the
same project. Use file or directory arguments for focused runs. `tsconfig.json`
checks every application and test source with `jsxImportSource: @solidjs/web`.

`vitest.config.ts` is part of the test contract:

- `environment: "jsdom"` provides a browser-like DOM, but it is not WebKitGTK or a Tauri runtime.
- `src/test/setup.ts` imports DOM mocks from `setup-dom.ts`, awaits English i18next initialization and disposes mounted Solid roots after each test. DOM setup provides jest-dom matchers, observers, animation frames, storage and the controlled suggestion worker.
- `globals: false` means every test imports `describe`, `it`, `expect`, `vi`, and other Vitest APIs explicitly.
- `css: true` makes Vitest process imported CSS instead of replacing every stylesheet with an empty stub.
- `pool: "forks"` runs tests in child-process workers. `maxWorkers: 2` limits concurrency because the suite is memory-heavy.
- `execArgv: ["--max-old-space-size=8192"]` gives each fork the configured Node heap limit. The `test` package script also gives the parent Vitest process the same limit.

Keep frontend tests next to the owning module in `__tests__`, using the existing `.test.ts`/`.test.tsx` naming. Pure services should use direct inputs and outputs. Models should use owned `renderHook` from `@solidjs/testing-library`; components should use Testing Library queries and `userEvent` where user behavior matters. Mock Tauri APIs and repository API modules explicitly with `vi.mock`, reset mock implementations and timers in `beforeEach`/`afterEach`, and clear storage when a test changes persistent browser state. Do not treat a mocked `invoke` assertion as proof that Rust accepts the payload; that requires a Rust contract test or desktop E2E.

Create JSX inside the renderer callback so it belongs to the test owner. Use
`src/test/renderSolid.tsx` when a controlled prop update must preserve the component's DOM
and local state. Its `rerender` publishes props on that same owner. Use `settle` from
`src/test/settle.ts` for explicit model-command batch boundaries, and deferred promises
for ordering async responses. Forced flushes belong in tests, not production orchestration.
Cleanup from `src/test/setup.ts` disposes mounted owners; assert listener/observer/timer
cleanup directly when that lifetime is the behavior under test.

The setup awaits `i18n.changeLanguage("en")`. Tests that change language or rely on persisted language must restore their state; the setup is not rerun before every individual test.

## Rust unit and integration tests

Rust unit tests live in `#[cfg(test)]` modules beside their implementation. This is the right layer for private helpers, parsing and normalization, database query semantics, validation, scheduler behavior, and service/command branches that do not require a running Tauri application.

Cargo discovers integration targets in `src-tauri/tests/`. Inspect the current target list with `cargo metadata --manifest-path src-tauri/Cargo.toml --locked --offline --no-deps --format-version 1`, selecting targets whose `kind` contains `test`. The focused package scripts select only `backend_integration` and `backend_e2e`; they do not cover every integration target.

The integration binaries use the public `media_tagger` library API. Examples include:

- `backend_integration.rs` covers file-backed asset/tag queries, root-prefix deletion and distinct thumbnail paths, complete library clearing, bulk tag merge, and bulk media-group replacement.
- `backend_e2e.rs` composes database and tag helpers into one backend workflow: export-shaped rows, CSV-style tag parsing/normalization/merge, filtering, group replacement, and library clearing. It does not invoke the CSV command parser itself.

Ignored diagnostics require explicit opt-in. For example, run `cargo test --manifest-path src-tauri/Cargo.toml --locked --test backend_runtime metadata_scale_measurements -- --ignored --nocapture --test-threads=1` for isolated memory/timing measurements, or use the [tag-save benchmark command](#tag-save-performance-regressions). Discovering or building these targets in the default suite does not execute their ignored tests.

Fixture and isolation patterns:

- Use the `tempfile` dev dependency and hold the returned `TempDir` for the full test. Its directory is removed on drop.
- Use `Connection::open_in_memory()` for database-only unit tests. Use `tempdir()/media.db`, `db::open_connection`, and `db::init_schema` when the file-backed behavior is part of the assertion.
- Create scan roots and media placeholders only below the test temp directory. Many backend tests need path identity and metadata, not decodable media, so small byte strings are intentional fixtures.
- When image decoding is the behavior under test, generate a valid image in the temp directory, as the scan-service tests do. Do not reuse a personal media library.
- Build ZIP/backup fixtures inside the temp directory and keep all restore destinations there. For filesystem-destructive code, assert both the database result and the surviving or removed files.
- Scheduler tests inject a processor and use synthetic paths; parser tests use literal tool output. They should not depend on a locally installed `ffmpeg` unless the test explicitly targets the executable boundary.

Cargo may execute independent tests concurrently. Never use a shared fixed database, app-data directory, scan root, output filename, or mutable process-global fixture. If a test truly changes process-wide state, serialize it explicitly or redesign it around injected state.

The Rust player regression generates a temporary MP4 with ffmpeg and drives the actual libmpv worker using null audio/video output. It verifies seeking, looping, replacement, settings retention, corrupt-file errors, and retry recovery without claiming GTK or OpenGL coverage. A separate test supersedes a committed session before `loadfile` and checks retirement without cancelling the replacement request.

The same worker regression runs 30 cycles of all six speed choices, alternating same-file reopening and two-file replacement. It checks autoplay from the beginning, advancing time, retained rate and rejection of stale controls, close and failure events. Real-libmpv flag tests cover true/false values, property errors and guarded C-integer storage. Run these with `cargo test --manifest-path src-tauri/Cargo.toml video_ --lib`. The native lifecycle suites live in `src/features/lightbox/video/__tests__`, covering session races, labels, bounds acknowledgment, fullscreen, retry, keyboard ownership and disposal. The native bounds helper and its tests live beside the video model.

## Packaging checks

`bun run test:packaging` verifies lockfile source generation, publication identity rejection, checksum inventory, duplicate-bundle rejection, and failed-build preservation with disposable artifacts. Docker builds perform format-specific artifact checks. See [package verification](linux-packaging.md) for installed-package tests and remaining desktop coverage.

## Real desktop E2E

Release production is documented in [setup and builds](setup-and-build.md#docker-packages). Its build does not run test suites.

Specs without an opt-in flag run on every `bun run test:e2e:tauri`: `app.smoke`, `app.workflows`, `group-order`, `popular-tags`, `review-fixes`, and `startup-scan`. The others skip unless their flag is `1`:

| Spec | Flag |
| --- | --- |
| `about` | `MEDIATAGGER_ABOUT` |
| `animations` | `MEDIATAGGER_ANIMATIONS` |
| `gallery-browsing` | `MEDIATAGGER_GALLERY_BROWSING` |
| `gallery.performance` | `MEDIATAGGER_GALLERY_PERF` |
| `gallery-selection` | `MEDIATAGGER_GALLERY_SELECTION` |
| `grid-baseline` | `MEDIATAGGER_GRID_BASELINE` |
| `lightbox-activity` | `MEDIATAGGER_LIGHTBOX_ACTIVITY` |
| `lightbox-images` | `MEDIATAGGER_LIGHTBOX_IMAGES` |
| `lightbox-recovery` | `MEDIATAGGER_LIGHTBOX_RECOVERY` |
| `native-video-escape`, `video-speed` | `MEDIATAGGER_NATIVE_VIDEO` (plus `MEDIATAGGER_NATIVE_DISPLAY`) |
| `search-recovery` | `MEDIATAGGER_SEARCH_RECOVERY` |
| `selection-recovery` | `MEDIATAGGER_SELECTION_RECOVERY` |
| `settings-language` | `MEDIATAGGER_SETTINGS_LANGUAGE` |
| `settings-maintenance` | `MEDIATAGGER_SETTINGS_MAINTENANCE` |
| `startup-scan-remount` | `MEDIATAGGER_STARTUP_REMOUNT` |
| `tag-editor` | `MEDIATAGGER_TAG_EDITOR` |
| `thumbnail-fade` | `MEDIATAGGER_THUMBNAIL_FADE` |
| `ui.redesign` | `MEDIATAGGER_UI_REDESIGN` |

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

The desktop harness discovers all `e2e/specs/**/*.e2e.js` files. Opt-in suites still require
their documented environment flags; explicit `--spec` flags restrict discovery. After
checking the native window title, each spec starts in English with the dark theme.
Reloads within a spec preserve preferences so persistence checks remain meaningful.

### Build, driver, and session order

`bun run test:e2e:tauri` performs these steps in order:

1. WebdriverIO `onPrepare` records the start time and validates the E2E build configuration.
2. It removes only the expected old E2E executable and calls the guarded E2E app-data cleanup.
3. `buildE2eApp()` validates again and runs `bun run build`, producing a fresh frontend in `dist/`. A failed build stops WDIO before starting workers.
4. The helper removes only the expected executable, sets `CARGO_TARGET_DIR` to the isolated target, and runs `tauri build --debug --no-bundle --config src-tauri/tauri.conf.e2e.json -- --locked`.
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

### Isolated desktop smoke

With the Linux prerequisites, Xvfb, xauth and `dbus-run-session` installed, run this
from the repository root in Bash. It creates temporary XDG directories and uses authenticated Xvfb with a private D-Bus
session. The existing harness still validates the E2E identifier, build target, cleanup
path and live title. Do not run another desktop controller or build concurrently.

```bash
set -euo pipefail
smoke_root=$(mktemp -d /tmp/mediatagger-doc-smoke-XXXXXX)
export XDG_DATA_HOME="$smoke_root/data" XDG_CONFIG_HOME="$smoke_root/config"
export XDG_CACHE_HOME="$smoke_root/cache" XDG_RUNTIME_DIR="$smoke_root/runtime"
mkdir -p "$XDG_DATA_HOME" "$XDG_CONFIG_HOME" "$XDG_CACHE_HOME" "$XDG_RUNTIME_DIR"
chmod 700 "$XDG_RUNTIME_DIR"
printf 'Temporary smoke profile: %s\n' "$smoke_root"
unset WAYLAND_DISPLAY
export GDK_BACKEND=x11 LANG=C.UTF-8 LC_ALL=C.UTF-8
export GIO_USE_VFS=local GTK_USE_PORTAL=0
dbus-run-session -- xvfb-run -a -s '-screen 0 1440x1100x24 -nolisten tcp' \
  sh -c 'export MEDIATAGGER_NATIVE_DISPLAY="$DISPLAY"; exec bun run test:e2e:tauri --spec e2e/specs/app.smoke.e2e.js --logLevel warn'
```

This smoke checks gallery/settings navigation, confirmation cancellation and persisted bulk
tag/group actions. Its input PNGs are public repository icons; source files are not deleted.
It does not prove native playback or native file pickers. Keep logs/screenshots in ignored
`artifacts/`. After the owned app/driver/display processes exit, check for session-owned FUSE
mounts under the printed temporary root, unmount any that remain, and remove only that root.
A failed run retains the root for inspection; never substitute a development/release profile.

### Fixtures, selectors, and destructive workflows

`app.smoke.e2e.js` indexes the checked-in PNG assets under `src-tauri/icons` and exercises navigation, settings, bulk tags, and bulk groups. It does not delete those source files. `app.workflows.e2e.js` creates unique roots with `fs.mkdtemp`, uses `e2e/fixtures.js` to copy valid PNG icons or use ffmpeg to generate a small GIF and two MP4 files without audio tracks, and records temporary CSV/ZIP artifact paths under the operating-system temp directory. Its `afterEach` clears the isolated library database and removes only those roots and artifacts. Video coverage includes picture clicks to pause/resume and focused Left/Right seeking by five seconds, plus native IPC checks for paused seeking, fullscreen session continuity, retained preferences, autoplay on replacement, and stale closes. These checks use backend snapshot attributes, not GTK button clicks or video pixels. Other video coverage checks authorized native opening, the full native-player footprint, absence of a DOM control rail, rapid source switching, absence of an HTML `<video>`, error fallback, and native-surface cleanup. WebDriver cannot inspect pixels or controls rendered by GTK above the WebView.

The workflow suite deliberately tests library clear, scan-root removal, backup restore, and permanent media deletion. Keep every deletable media fixture under a newly created temp root. Never change a destructive spec to index a personal directory, the repository root, or production app data. Do not weaken the identifier, target-directory, app-data-path, or live-title guards to make a failing run proceed.

Desktop selectors currently mix stable IDs/classes/data attributes, `aria-label` values, visible English text selectors such as `button=Apply`, and a small number of XPath expressions. Prefer stable accessible roles/names or explicit test IDs when text is not itself the behavior. If visible copy or accessibility labels change, update the associated E2E selectors in the same change.

After checking the live E2E title, the desktop harness sets only `media-tagger.language=en` and `media-tagger.theme=dark`, then reloads before each spec. It leaves unrelated storage untouched. A reload inside a spec retains any language/theme changes, allowing persistence assertions. The tooling tests verify that an unexpected title prevents preference writes.

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
`MEDIATAGGER_GALLERY_PERF=1` suite exercises 2,048 images and cache eviction. It also checks selection summary IPC and full-selection metadata writes after eviction and filter changes.

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
immediate full opacity and no entrance animation on gallery images, including newly
mounted rows. Tile backgrounds and the gallery container stay opaque. The filename and
opt-in flag retain their historical names. Screenshots and samples remain under
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

`e2e/specs/video-speed.e2e.js` is opt-in with `MEDIATAGGER_NATIVE_VIDEO=1`. It uses generated MP4s and native XTest pointer/key events through `e2e/native-input.py`. WebDriver reads session snapshots and supplies DOM geometry; it does not click GTK controls. The spec checks outside dismissal over the picture, controls, sidebar and backdrop, trigger toggling, Escape before fullscreen, focus loss, zero bounds, stale bounds, and 30 speed-change/close/reopen cycles across two videos. Some cycles explicitly close the backend session while the menu is open to check teardown. A recovery case corrupts only its generated original, checks the failure UI, restores the bytes, clicks Retry and compares decoded frames. Mixed-media navigation also checks that leaving fullscreen video restores the GIF windowed layout.

`e2e/specs/native-video-escape.e2e.js` uses the same opt-in and private-display setup.
It proves GTK play focus with Space changing playback and GTK volume focus with
End/Home changing volume, while DOM input counters remain empty. Escape must then
close the windowed preview. Countercases cover WebView focus, speed-menu dismissal,
fullscreen exit, inline delete confirmation and the narrow drawer. Screenshots and
observations stay under `artifacts/native-video-escape/`. Run it by substituting this
spec path in the command below. Session retirement, higher registered layers and
pending deletion also have controlled frontend/Rust tests; desktop input cannot be
established by dispatching a DOM `KeyboardEvent`.
The drawer trigger and cleanup close use the native pointer helper to reveal fading
controls and wait for hit-testable geometry before clicking. The helper approaches from
an adjacent pixel so it still generates movement when the pointer already rests on the
target. A native regression parks the pointer over each floating button, waits for zero
opacity and disabled pointer input, then opens the drawer and closes the preview through
that helper. An unchanged pointer coordinate must not be treated as user activity.

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
Presentation model tests cover the exact three-second deadline, movement filtering, pointer presses,
wheel/scroll/keyboard resets, native activity callbacks, touch visibility and cleanup.
Component tests cover pending-deletion guards and the first navigation shortcut after idle.

### Persistent desktop control

`e2e/control.js` reuses the E2E build configuration and shared media fixtures without running
the Mocha suite. The [MediaTagger verification skill](../../.claude/skills/verify-mediatagger/SKILL.md) (`/verify-mediatagger` in Claude Code)
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

## Gallery performance measurements

Run the complete opt-in suite in its declared order, using a private Xvfb/D-Bus session and disposable XDG directories as described under [real desktop E2E](#real-desktop-e2e):

```sh
MEDIATAGGER_GALLERY_PERF=1 bun run test:e2e:tauri --spec e2e/specs/gallery.performance.e2e.js
```

Do not use a single-case filter to verify cache provenance. The timing case must precede the stable-metadata traversal, GIF check and selection scenario. The fixture contains 2,048 PNGs, 12 GIFs and two videos. Library and thumbnail clearing precede indexing; startup/filtering then prepares some previews. Timing uses only images, at indices 128 through 1920 in steps of 128 and then 2047. The same route and filter apply to the revisit. The initial mounted area and untimed returns to 128/0 are excluded from first-use statistics.

`artifacts/gallery-performance/timings.json` records preparation, initial visible/mounted IDs, initial final-JPEG filenames, route/filter, per-scroll file inventories, visible IDs/filenames, raw elapsed times and animation-frame intervals. It classifies samples by whether none, some or all visible JPEGs existed before the scroll. Only the first category enters the first-use summary, and at least one such sample is required. Every revisit sample must reuse existing files. Raw arrays retain order; percentile calculation sorts a copy. File inventories are outside the measured intervals.

The observations measure scroll-to-visible-JPEG-decode with WebDriver/polling overhead. They exclude the subsequent opacity fade and do not flush the OS cache. First-use refers to preview files absent at the pre-scroll snapshot, not uncached source reads or proof that no render was queued. Revisits cross the 12-page frontend cache bound, so they are not promises of resident query pages or decoded images. Keep build profile, viewport, route, fixture and host conditions comparable; this suite has no portable latency threshold. Screenshots and measurements remain ignored local artifacts. See [gallery behavior](../subsystems/library-query-and-gallery.md#gallery-performance-verification) and [thumbnail reuse](../subsystems/thumbnails.md#cache-provenance-in-gallery-measurements).

## Grid-performance baseline

The [grid-performance plan](../plans/grid-performance/README.md) records numbers before and after its stages in the results table of [stage 07](../plans/grid-performance/07-verification-and-docs.md#wyniki). Two opt-in measurements produce them. Neither has thresholds; compare runs made with the same build profile on the same idle host.

`src-tauri/tests/grid_baseline.rs` contains three ignored diagnostics. Run them alone, in release mode, with `TMPDIR` on a disk-backed filesystem, because a tmpfs `/tmp` hides SQLite write costs:

```sh
TMPDIR=/var/tmp cargo test --manifest-path src-tauri/Cargo.toml --locked --release \
  --test grid_baseline -- --ignored --nocapture --test-threads=1
```

- `query_rebuild_and_page_baseline` seeds synthetic 100,000- and 1,000,000-row libraries with SQL. The rows contain kinds, favorites, four-item media groups, 2,000 vocabulary tags, a `common` and a `rare` tag, a scan-root mapping and pseudo-random modification times, 0.1% of them zero. It bumps the revision before each `AssetQueryManager::start` call, so every sample rebuilds the ID vector. The filters are none, favorites, an included tag, a rare tag, an excluded tag and a media kind. It also times cached starts and 128-item pages.
- `mutation_latency_and_rebuilds_during_scan_baseline` times the revision-aware favorite and tag writes through the database runtime, first idle on 100,000 rows. It then repeats them during ten `scan_folder`-shaped scans of 20,000 new small JPEGs. During each scan, a simulated user writes every 50 ms, and a simulated gallery reads a page every 100 ms and rebuilds the session whenever the page is stale. The output reports scan duration, revision bumps, rebuilds, latency distributions and failed writes.
- `jpeg_24mp_thumbnail_baseline` generates a 6000×4000 JPEG and times `thumbs::create_image_thumb`.

Each output line starts with `[baseline]` and contains only counts and timings. Rust-side samples exclude IPC, serialization and the WebView.

`e2e/specs/grid-baseline.e2e.js` is opt-in with `MEDIATAGGER_GRID_BASELINE=1`. Run the whole spec, because its three cases share one library and must run in order. It uses the isolated desktop recipe above, and setting `TMPDIR` to a disk-backed directory keeps the media and app data off tmpfs:

```sh
MEDIATAGGER_GRID_BASELINE=1 MEDIATAGGER_PERF=1 bun run test:e2e:tauri --spec e2e/specs/grid-baseline.e2e.js
```

With ffmpeg, the spec generates 240 browse JPEGs and 20,000 scan JPEGs at 320×240, plus the shared GIF and two MP4 fixtures. The scan files get modification times spread over 2015–2023, so the browse images stay first in the gallery. It indexes the browse folder and wraps the IPC fetch bridge to record command timings.

1. A single 20,000-file scan takes about two seconds in this build, while each preview operation takes about a second of WebDriver input. So the page runs a loop that alternates `scan_folder` on the 20,000 files with `remove_scan_root`, and the loop ends after a scan. While the loop runs, the spec opens 20 previews. In each one it types a new tag, presses Enter and immediately clicks the favorite button with WebDriver input. An in-page frame loop timestamps the input and the moment the UI shows the tag chip and the favorite state. Each operation is then labelled with its load phase (scan, remove or between) and classified from the UI, the IPC trace and the persisted details as failed, without IPC, delayed (over one second) or successful. The favorite button changes only after the backend succeeds. The tag chip, however, appears at once as a pending intent, so a tag counts as confirmed only when its `set_asset_tags` call returns. The spec also counts `start_asset_query` calls and stale pages during scan phases.
2. It renders all thumbnails, reloads, and drives scripted decaying flings and jumps by setting `scrollTop` on animation frames. For each one it records the time from stop to complete visible tiles, frame gaps in the following second, frames with `data-scroll-paused` set, mounted tiles and grid DOM nodes. WebKitGTK 2.54 does not expose `longtask` entries, so frame gaps come from `requestAnimationFrame` intervals.
3. It clears thumbnails, reloads and jumps to six cold positions. For each visible tile it records the time from the jump to its first real thumbnail load.

`artifacts/grid-baseline/<timestamp>/results.json` keeps the raw samples, and screenshots are stored next to it. The desktop build is the unbundled debug E2E profile, so backend work in this spec runs unoptimized. The scripted flings are DOM scrolling, not native kinetic input, and Xvfb uses software rendering.

## Choosing the test level

For documentation-only changes, verify local links/anchors, referenced commands and symbols, and `git diff --check`. Run an existing focused test when it resolves a behavior claim. An explicitly requested workflow-validation stage, such as Solid migration stage 10, also runs its documented build and isolated smoke recipe. Ordinary prose edits do not require release builds or destructive desktop tests.

| Change | Minimum focused proof | Add when applicable |
| --- | --- | --- |
| Pure TypeScript service, parser, or reducer | Vitest unit test | Component/model test if rendering or Solid ownership matters |
| Solid component, model, keyboard behavior, modal, or accessibility contract | Vitest with Testing Library | Desktop E2E for behavior dependent on WebView/Tauri or a high-value full workflow |
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
- Specs start in English/dark after the title guard; translated scenarios must switch language explicitly.
- An `EPERM` during E2E app-data deletion leaves stale isolated data and does not fail fast.
- Desktop runs share `dist/`, the E2E target and driver port. Run them serially, including standalone E2E builds.
- Native video playback depends on libmpv, GTK, and OpenGL. WebDriver sees the native session container and backend snapshot attributes, but not decoded `GtkGLArea` pixels or the native GTK control overlay.
- Backend fixtures cover many filesystem semantics but do not constitute exhaustive testing on every supported filesystem or operating system.

## Troubleshooting

| Symptom | Check and resolution |
| --- | --- |
| Cannot connect to `127.0.0.1:4444`, address already in use, or driver exits immediately | Stop stale `tauri-driver`/WebDriver processes and confirm port 4444 is free. Only one configured suite can use the endpoint at a time. |
| Session creation fails because the native driver is missing | Install WebKitWebDriver or point `WEBKIT_WEBDRIVER_PATH` at it. |
| `tauri-driver` is not found | Install it with Cargo or set `TAURI_DRIVER_PATH` to the full executable path. The default is `~/.cargo/bin/tauri-driver`. |
| E2E app data cannot be removed or old state appears | Close the E2E application and stale drivers, then rerun. An `EPERM` warning means cleanup was skipped; inspect only the isolated `com.example.mediatagger.e2e` directory, never delete production app data as a workaround. |
| Standalone E2E build shows old frontend behavior | Both E2E entry points build `dist/` first. Check that the run reached a successful frontend build and launched the expected E2E executable; do not run another build concurrently. |
| E2E build refuses its identifier, title, or target | Restore `src-tauri/tauri.conf.e2e.json` and `e2e/build-e2e.js` to the exact isolated values. The refusal is a safety check, not a cache problem. |
| Expected English heading/button is missing | Verify the isolated app's selected language. The specs use English text and `aria-label` selectors; the harness should have reset language after its title guard; check whether the spec changed it. |
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

The focused frontend regressions are `src/features/settings/__tests__/startupScan.test.tsx`
and `src/app/__tests__/startupScanRemount.test.ts`. Runtime tests defer completion across
replacement with zero, fully cached and partially cached rows, cover retained success and
failure, and await the actual startup action before checking disposal. Native tests in
`app/state.rs` and `commands/scan.rs` cover launch-snapshot contention, shared waiting,
abandoned observers, retained success/error, worker panic and no-work launches. Existing
Rust tests also cover schema migration, preference persistence and backup compatibility.

On Linux, opt into a real remount during native scanning:

```sh
MEDIATAGGER_STARTUP_REMOUNT=1 bun run test:e2e:tauri -- --spec ./e2e/specs/startup-scan-remount.e2e.js
```

Use the private display and temporary XDG directories from the isolated desktop recipe.
The spec copies the guarded E2E binary to its own temporary directory and places a gated
`ffprobe` wrapper beside it. The wrapper holds actual video metadata probing across a
WebView reload, then delegates to `/usr/bin/ffprobe`. No application response is mocked.
The spec checks gallery counts changing from 0 to 1 and 2 to 3 without another UI action,
retained completion after another reload, one probe invocation and no gate timeout.
Screenshots and result JSON remain under `artifacts/startup-scan-remount/`; the spec returns
to the original E2E binary and removes its temporary executable, wrapper and media.
This test requires `/usr/bin/python3`, `/usr/bin/ffmpeg` and `/usr/bin/ffprobe`.

## Backend runtime regressions

`backend_runtime.rs` exercises precise fingerprint/version coupling, version-2 failure migration, dedicated FULL-synchronous connections, and a real 514-file scan across batching and overlapping roots. Unit tests cover maintenance admission/draining, pool callback cleanup, query cache budget/epoch, stale thumbnail stream publication and frontend reload, directory-sync failures after successful moves, disk-staged CSV row order/clears, and bounded subprocess diagnostics/timeouts.

Run the opt-in metadata measurements alone so SQLite's process-wide allocator counters are attributable:

```sh
cargo test --manifest-path src-tauri/Cargo.toml --test backend_runtime -- --ignored --nocapture --test-threads=1
```

The fixtures contain 1,000 and 10,000 metadata rows and no media collection. The output distinguishes Rust ID-vector capacity from additional peak SQLite allocation and checks indexed filename lookup plus seven-row thumbnail pages.

## Review fix regressions

`e2e/specs/review-fixes.e2e.js` uses disposable media to verify navigation after a group-order save beyond the first 128-item page, literal tag include/exclude and autocomplete, and thumbnail clearing after a changed-file rescan. The order fixture uses positive integers with gaps so a valid integer edit moves an asset between its neighbors. It saves screenshots and the clear/regeneration result under `artifacts/app-review/2026-09-06/fix-verification/`. Run it with the isolated desktop harness:

```sh
bun run test:e2e:tauri -- --spec ./e2e/specs/review-fixes.e2e.js
```

Lower-layer regressions are `src/features/lightbox/__tests__/createLightbox.test.ts`, `literalTagSearch.test.ts`, the library model/API tests, the query-position service test, and `src-tauri/tests/thumbnail_clear.rs`. The Rust file-backed test also checks nested obsolete files, skipped symlinks, exclusive-reader coordination, failure-marker removal, source preservation, and actual image regeneration.

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


## Shared tag editor regressions

`TagEditor.test.tsx` covers composition, blank-space focus, disabled/loading guards,
controlled exclusion updates and keyboard focus. `BulkActionsSidebar.test.tsx` uses
deferred saves to check that success, a failed result and rejection from a retired
selection cannot erase a new draft or move focus. Save-completion tests also cover
waiting for the enabled input to commit and canceling pending focus on selection changes. `LightboxModal.test.tsx` checks that
Escape dismisses suggestions while retaining the lightbox, draft and input focus.

`e2e/specs/tag-editor.e2e.js` is opt-in with `MEDIATAGGER_TAG_EDITOR=1`. It requires
a disposable XDG profile and an authenticated private Xvfb display, with
`MEDIATAGGER_NATIVE_DISPLAY` set to that display. Use the same isolation setup as
[native speed verification](#native-speed-menu-and-repeated-playback), then run:

```sh
GDK_BACKEND=x11 MEDIATAGGER_TAG_EDITOR=1 dbus-run-session -- xvfb-run -a -s '-screen 0 1440x1100x24 -nolisten tcp' sh -c 'export MEDIATAGGER_NATIVE_DISPLAY="$DISPLAY"; bun run test:e2e:tauri --spec e2e/specs/tag-editor.e2e.js'
```

The spec copies two disposable PNGs and seeds long/many tags through the backend.
Behavior checks use the visible editors: native clicks on field padding, keyboard
submission, suggestion selection and removal. Geometry and computed styles cover
both editors, both themes, 600px and 1100px widths, height limits, wrapping,
input visibility and focus feedback. Read-only details calls check persisted writes,
including a multiple-selection addition, followed by a frontend reload. The test also observes real Tauri custom-protocol fetches and cloned responses: existing-name additions and shared-name removals in both editors issue zero vocabulary-page requests, while new/last-reference names issue one page for the small fixture. Multi-selection merges verify both flag values. Counting begins after a rendered
suggestion proves vocabulary initialization. Assertions wait for the editor to
finish, then for all observed vocabulary fetches to settle; the cloned save
response alone is not a completion barrier. The fetch observer is restored after
the scenario and in cleanup. Both assets are read again after the frontend reload.
`vocabulary-ipc.json` records requests and canonical responses.
Evidence remains under `artifacts/tag-editor/<timestamp>/`. The bottom-edge suggestion
placement limitation remains a separate issue; the popup assertion uses a desktop
viewport with room below the input.

## Tag-save performance regressions

`src-tauri/tests/tag_vocabulary.rs` uses disposable file-backed databases to verify
vocabulary flags, shared and last-reference removals, unrelated orphans, retained
IDs, legacy renames/collisions, count repairs, missing/empty inputs, revisions, and
rollback after a failed revision write. It checks idempotent index creation on new
and existing schemas. Query-plan assertions use 1,000 tags, because SQLite may
prefer a scan on a tiny table, and require indexed normalization and candidate
lookups. Existing `db.rs` tests also cover query impact and shared mapping repairs.

`src/library/vocabulary/__tests__/createKnownTags.test.ts` covers complete pagination, stable array identity,
changed publication, overlapping changes, unchanged saves joining an active pass,
failed-refresh retry, reset, and unmount. API and save-path tests cover both flag
values. Save mocks and deferred responses in `src/app/__tests__/runtimeIntegration.test.ts` and the affected model tests derive their types from the API or action signatures, so
missing required fields fail `bun run typecheck`. Runtime integration cases defer saves to verify shared metadata ownership and maintenance;
vocabulary tests check refresh counts, and sidebar tests check mounted editor state and focus. `SuggestionClient.test.ts` composes the real vocabulary model and worker
client with a controlled worker: twenty unchanged saves after hydration add zero
page requests and zero worker vocabulary messages.

Run the focused file-backed tests and the diagnostic benchmark with:

```sh
cargo test --manifest-path src-tauri/Cargo.toml --locked --test tag_vocabulary
cargo test --manifest-path src-tauri/Cargo.toml --locked --test tag_save_benchmark -- --ignored --nocapture
```

The benchmark calls the real revision-aware single and bulk helpers on disposable
WAL databases. It crosses 1,000/10,000/50,000 vocabulary names with 1/15/30 initial
tags per target asset and 1/4/16 background assignments per vocabulary name. Each
row reports the mean of twelve calls in microseconds, excluding fixture creation.
Bulk calls process ten assets. Existing-name additions increase target counts over
the twelve calls. All vocabulary names remain referenced by background assets.
There are no timing thresholds. Keep build profile and host conditions the same
when comparing revisions. See [recorded results](tag-save-benchmark.md).

### Tag-save verification on 2026-10-03

`bun run test:all` completed with exit 0 on Arch-based Linux using a private Xvfb
and D-Bus session, disposable XDG directories and `MEDIATAGGER_TAG_EDITOR=1`.
Quality checks, 699 frontend tests, 206 Rust tests and 28 desktop tests passed.
The desktop runner discovered 16 spec files; 77 cases behind other opt-in flags
were skipped. Two Rust diagnostics were ignored in the full suite; the tag-save
benchmark runs separately as described in [the benchmark report](tag-save-benchmark.md).
The configured RustSec exclusions and allowed warnings were unchanged.

The tag-editor scenario observed 18 successful saves: thirteen responses with
`vocabulary_changed=false` caused zero extra vocabulary pages; five responses with
`true` caused exactly one page each. The final observer state recorded five pages,
eighteen responses and zero pending pages. Both assets retained the multi-selection
tag after frontend reload. The five tag-editor cases also passed independently.

The desktop run exposed two issues corrected before the successful full rerun.
Bulk save focus restoration now prevents parent scrolling while `TagEditor`
reveals the input inside the field. The group-navigation regression now uses
spaced integer orders, matching the current editor validation, while retaining
the same neighbor-navigation assertions beyond the first page.

To reproduce from the repository root, with the prerequisites above installed:

```sh
scratch=$(mktemp -d /tmp/mediatagger-tag-save-XXXXXX)
export XDG_DATA_HOME="$scratch/data" XDG_CONFIG_HOME="$scratch/config"
export XDG_CACHE_HOME="$scratch/cache" XDG_RUNTIME_DIR="$scratch/runtime"
mkdir -p "$XDG_DATA_HOME" "$XDG_CONFIG_HOME" "$XDG_CACHE_HOME" "$XDG_RUNTIME_DIR"
chmod 700 "$XDG_RUNTIME_DIR"
unset WAYLAND_DISPLAY
export GDK_BACKEND=x11 LANG=C.UTF-8 LC_ALL=C.UTF-8 MEDIATAGGER_TAG_EDITOR=1
dbus-run-session -- xvfb-run -a -s '-screen 0 1440x1100x24 -nolisten tcp' \
  sh -c 'export MEDIATAGGER_NATIVE_DISPLAY="$DISPLAY"; bun run test:all'
```

The spec removes its synthetic media roots. After the desktop processes exit,
unmount any session-owned FUSE mounts below `$scratch/runtime/doc` and
`$scratch/runtime/gvfs` before removing the scratch directory. Desktop screenshots and IPC evidence stay local under
`artifacts/`; keep redirected command logs local as well. No media or raw desktop
artifacts accompany this report.

## Solid feature verification

Migration-only shell/reference specs and renderer switches have been removed.
`bun run test:build` checks the actual production module graph for the Solid entry and
absence of React, its adapters and the removed source trees.

`src/test/renderSolid.tsx` updates props under one component owner; `settle.ts` supplies
explicit batch boundaries. Real virtual-core tests retain 10,000/26,000-slot limits,
measurement identity, null-ref startup, resize anchoring and cleanup. Query/cache tests
exercise page eviction and per-asset thumbnail updates. Model tests use deferred IPC for
canonical publication, contention, partial responses and disposal.

Run opt-in suites under the private display and disposable XDG profile described above:

```sh
MEDIATAGGER_GALLERY_BROWSING=1 bun run test:e2e:tauri --spec e2e/specs/gallery-browsing.e2e.js
MEDIATAGGER_TAG_EDITOR=1 MEDIATAGGER_GALLERY_SELECTION=1 bun run test:e2e:tauri \
  --spec e2e/specs/tag-editor.e2e.js --spec e2e/specs/gallery-selection.e2e.js --spec e2e/specs/group-order.e2e.js
MEDIATAGGER_SETTINGS_MAINTENANCE=1 bun run test:e2e:tauri --spec e2e/specs/settings-maintenance.e2e.js
```

Gallery browsing indexes 2,048 PNGs plus GIF/video fixtures, crosses 16 pages, returns to
evicted ranges and records geometry at 1440/600/320px in English/Polish and both themes.
Tag-editor tests verify persisted writes, vocabulary IPC counts and focus. Maintenance
holds delivery of a real scan response across settings remount, and verifies mixed duplicate
batches against files and reloaded rows. Native chooser interaction is mocked in frontend
tests; desktop CSV/bundle roundtrips call real commands directly.

`MEDIATAGGER_REDUCED_MOTION=1` requires the actual WebView preference, configured through the
isolated GTK profile. Run animation, thumbnail, activity and maintenance variants with it.
Screenshots, timing samples and raw logs stay ignored under `artifacts/`.


## Lightbox page recovery and late image loads

`MEDIATAGGER_LIGHTBOX_RECOVERY=1` enables `e2e/specs/lightbox-recovery.e2e.js`.
Run it with the private Xvfb display and disposable XDG profile described above:

```sh
MEDIATAGGER_LIGHTBOX_RECOVERY=1 bun run test:e2e:tauri --spec e2e/specs/lightbox-recovery.e2e.js
```

The spec indexes 132 copied PNG fixtures. It changes the requested offset of the second
page to an invalid unsigned value at the fetch boundary, producing a real native rejection.
It verifies cached Previous, visible lightbox Retry, exactly one fresh page request after
restoring the payload, and navigation from index 127 to 128. A second case retains image A,
navigates to loaded image B, then assigns a synthetic SVG source to detached A to deliver
an actual WebKit load event. B retains its fitted dimensions and zoom, and subsequent zoom
and reset still target B. This controls late DOM delivery, not the original asset protocol's
network or decode timing. Screenshots and JSON observations stay in ignored
`artifacts/lightbox-recovery/`; media and isolated app data are disposable.

The focused model/component tests in `createLightbox.test.ts`, `imageControls.test.tsx`
and `createAssetQuery.test.ts` cover prefetch failure, repeat Retry failure, selection/close/
refresh/disposal races, target-only page recovery, ID/path image replacement, inactive
owners and pan after late load. Run the affected frontend layer with:

```sh
bun run test -- src/features/lightbox src/library/query/__tests__/createAssetQuery.test.ts src/app/__tests__/lightboxRecovery.test.tsx
```


## Selection retry after layout changes

`MEDIATAGGER_SELECTION_RECOVERY=1` enables `e2e/specs/selection-recovery.e2e.js`.
Run with a private Xvfb display and disposable XDG profile as described above:

```sh
MEDIATAGGER_SELECTION_RECOVERY=1 bun run test:e2e:tauri --spec e2e/specs/selection-recovery.e2e.js
```

The spec indexes 2,052 copied PNG fixtures and traverses more than twelve query pages.
It rejects an evicted offscreen page by sending a negative unsigned offset at the fetch
boundary. Native Shift-click and rectangle selection must retain their previous IDs,
render the gallery error banner, survive its actual ResizeObserver geometry change, and
complete the original additive range through the visible Retry button. It records page
requests and measured grid margins under ignored `artifacts/selection-recovery/`.
The scroll helper allows one pixel at viewport edges because WebKit rounds `scrollTop`
while active rectangle scrolling preserves fractional offsets. A failure records the
last forty scroll attempts, including requested index, actual offsets and tile bounds.
Frontend regressions in `GalleryGrid.test.tsx`, `createGridHandlers.test.tsx` and
`createSelection.test.ts` cover the integrated query/selection/banner path, pending reads,
live gesture cancellation, new input, mode/query changes, identity reset and disposal.


## Million-result gallery layout

Run `sh e2e/gallery-scale/run.sh` from the repository root. It builds the actual
`GalleryGrid`, virtual-core, selection model and production CSS into a standalone
fixture, then opens an ephemeral WebKitGTK 4.1 view on a private Xvfb display.
Python needs PyGObject and GTK 3/WebKit2 4.1 typelibs. Native input uses libX11 and
libXtst; screenshots require ImageMagick `import`. The runner uses temporary XDG
folders, a loopback fixture server and its own display. It does not attach to Tauri
or read the user's library. Do not run `webkit.py` against a normal desktop.

Lazy synthetic summaries exercise one million results at one, five, six and eight
columns, narrow width, tile-size changes, viewport-height changes and resize anchors.
Assertions require final index 999999 to be visible, demanded and selected by native
clicks, with a spacer no taller than 4,000,000 pixels and fewer than 120 mounted tiles.
XTest drives the actual scrollbar thumb, wheel, Shift-click and rectangle selection.
The fixture checks global selection IDs and range reads, then disposes the gallery
and checks that further scroll/resize events publish no ranges. It also includes
an uncompressed 100-result control. Screenshots, JSON results and profile/server
cleanup evidence remain ignored under `artifacts/gallery-scale/`.

This establishes native layout and input behavior with synthetic data. Range callbacks
represent page demand; there is no million-row SQLite query, thumbnail rendering or
performance comparison. The integrated `GalleryGrid.test.tsx` regression separately
runs the real sparse page cache with a mocked transport, demands offset 999936 and
checks the final tile, bounded cache and footer total. Footer tests exercise all four
thumbnail-generation/`hasMore` combinations. The production caller passes
`query.offset < query.total`; sparse viewport loading remains active after `hasMore`
becomes false. For live media/IPC and timing provenance use the separate gallery
browsing and performance suites above.

```sh
bunx tsc --noEmit -p e2e/gallery-scale/tsconfig.json
bun run test -- src/features/gallery src/app/__tests__/gallery.test.tsx src/library/__tests__/createLibrary.test.ts src/library/query/__tests__/createAssetQuery.test.ts
sh e2e/gallery-scale/run.sh
```

## Fast gallery scrolling

`createVirtualGrid.test.tsx` covers the half-viewport threshold, cumulative logical
distance across reversals, slow motion, the 150 ms idle deadline, unchanged final
ranges, geometry anchors, disposal and the near-end loading guard. The integrated
`GalleryGrid.test.tsx` checks that paused slots contain no images or animations,
have no thumbnail subscriptions, skip passing pages and resume the final range.
Library tests retain late pages and independent selection/lightbox reads through
pause and filter changes. Thumbnail queue tests preserve dispatched results,
cached paths and additive previews when gallery demand is cleared.

Run the gallery and library suite, type checks and native scale fixture with the
commands in [million-result gallery layout](#million-result-gallery-layout).
The native fixture waits past the pause deadline before inspecting loaded tiles.

For a separate native timing run:

```sh
GALLERY_PERFORMANCE=1 sh e2e/gallery-scale/run.sh
```

`performance.tsx` uses the actual gallery, sparse page cache and thumbnail queue,
with 26,000 synthetic summaries and generated SVG thumbnails. The Tauri transport
is mocked, with 12 ms page replies and 20 ms thumbnail batches. XTest drags the real
scrollbar thumb in sixty steps spaced 20 ms apart, repeating three times after
query/cache reset. Both variants must reach index 25999. The script records rAF
intervals during motion and the first 150 ms after the last position change,
page/thumbnail invocation counts, requested thumbnail IDs, and time from the last
scroll event to a loaded visible image and to image opacity above 0.95. It also
checks that every paused frame has no images, pulsing placeholders or spinners.

For before/after comparison, `GALLERY_BASELINE_DIR` can point to saved earlier
copies of `createVirtualGrid.ts`, `GalleryGrid.tsx` and `createLibrary.ts`. The Vite
fixture substitutes those three files while using the same remaining source,
CSS and synthetic transport. Save these files before editing them, and preserve
`artifacts/gallery-scale/result.json` after each run because the next run replaces
it. Run both variants without concurrent builds or test suites. This is a frontend
comparison on software-rendered Xvfb, not a SQLite, native IPC or real-media decode
benchmark. Results include the pause's deliberate delay. Gallery images now appear at full opacity
as soon as they load; the historical comparison below was recorded with the fade enabled.

The runner leaves raw samples, request traces, screenshots and profile/server
cleanup evidence in ignored `artifacts/gallery-scale/`. The performance screenshot
is `performance-thumbnails.png`. The `GALLERY_BASELINE_DIR` substitution is confined
to this fixture's Vite configuration and does not affect application builds.

The initial fast-scroll verification on 2026-10-17, before removal of the gallery
image fade, used WebKitGTK 2.54.1 on private Xvfb, with three passes
per variant and no concurrent tests. Both variants stopped at the same physical
scroll offset, 1,611,408 pixels, with the final result visible.

| Metric per drag | Before | After |
| --- | --- | --- |
| rAF interval p95 | 21 to 23 ms | 17 ms |
| Largest rAF interval | 22 to 28 ms | 17 to 18 ms |
| Page requests | 56 to 62 | 2 |
| Thumbnail batch requests | 17 to 19 | 1 |
| Thumbnail IDs requested | 756 to 860 | 28 |
| First loaded visible image after stopping | 33 to 85 ms | 233 to 238 ms |
| Visible image opacity above 0.95 after stopping | 295 to 341 ms | 489 to 490 ms |

The final two page reads were offsets 25856 and 25984, both in the current overscan
range. An intermediate run exposed an unrelated offset-128 read through the old
sequential near-end callback. The production caller now uses
`library.handleGalleryReachEnd`, which ensures only the current range; the integrated
million-result regression rejects earlier offsets. The timing fixture uses that
callback when available and retains the old callback only for saved baseline sources.
All 242 sampled paused frames across the final three passes had no images or animations.
The longer image delay is the cost of the 150 ms pause plus queueing, loading and fade.
These samples show reduced frontend work and more regular frames in this fixture;
they do not establish timings for real media collections or GPU-rendered desktops.

The focused run passed 217 tests across 19 files, application and fixture type checks,
Biome lint and the production module-graph build check. The native million-result suite
passed all ten layout/input scenarios. Baseline/final timing samples are saved locally
as `performance-before.json` and `performance-after.json`, and scale results as
`scale-result.json`, in `artifacts/gallery-scale/`. Both timing runs and the scale run
confirmed removal of their temporary profiles and shutdown of their fixture servers.
No Rust or full Tauri desktop suite was run for this frontend-only change.


A follow-up on 2026-10-17 removed `fadeIn` from `GalleryTile` after reproducing dim
incoming rows below the fast-scroll thresholds. The timing fixture now also drives
forty actual DOM scroll-position changes of 240 pixels at 60 ms intervals in a
600-pixel viewport. It samples decoded visible images each animation frame before
the native scrollbar scenarios. This paced scenario uses DOM scrolling in WebKitGTK,
not native wheel input; the existing three scrollbar drags still use XTest.

Before the fix, 114 of 154 frames contained dimmed, already loaded images, with up
to eight affected images and minimum opacity 0.574. After the fix, none of 156 frames
contained dimmed images. Neither run activated the fast-scroll pause during this
scenario. After native scrollbar dragging, loaded and fully visible image times
now coincide at 223 to 240 ms after the last scroll event; p95 frame intervals stayed
at 17 ms, with two final-range page requests and one thumbnail batch per drag.
Raw results and screenshots are in `artifacts/gallery-scale/fade-before.json`,
`fade-after.json`, `moderate-scroll-before.png` and `moderate-scroll-after.png`.
This follow-up used the isolated WebKitGTK fixture; the updated full-Tauri
`thumbnail-fade.e2e.js` expectations were not run in this check.
