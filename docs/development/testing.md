# Testing

Implementation entry points: [Vitest configuration](../../vitest.config.ts), [frontend test setup](../../src/test/setup.ts), [CI workflow](../../.github/workflows/quality.yml), [desktop harness](../../e2e/wdio.conf.js).

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
| `bun run test:app-control` | Runs Node tests for controller input validation, local-port conflicts, temporary-directory ownership, process-tree cleanup, and shared PNG/GIF/MP4 fixtures. Requires Linux local sockets and ffmpeg. |
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

There is no numeric coverage threshold. CI runs the complete frontend and backend suites, but coverage output is not a merge gate.

## Continuous integration

`.github/workflows/quality.yml` runs on pull requests and pushes to `main`. The frontend job uses Node `22.22.0`, Bun `1.4.0`, and `bun install --frozen-lockfile`; it validates locales, generator tests, TypeScript application/configuration projects, Biome, production dependencies with `bun audit`, Vitest, and the production frontend build. The Rust job uses `rust-toolchain.toml`, `cargo fetch --locked`, pinned `cargo-audit 0.22.0`, rustfmt, locked Clippy with warnings denied, locked Cargo tests, and the RustSec gate matching `audit:rust`. The script and workflow currently exclude `RUSTSEC-2026-0194` and `RUSTSEC-2026-0195`; they contain no rationale for those exclusions. Do not treat an excluded advisory as proof of safety.

CI does not launch an unqualified Tauri development or release profile and does not touch production app data. Real desktop E2E uses the isolated `.e2e` profile and must be run explicitly on a configured Linux host. The Docker release build does not run tests.

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

## Real desktop E2E

Release production is documented in [setup and builds](setup-and-build.md#containerized-arch-linux-release). Its build does not run test suites.

### Linux prerequisites

1. Install the repository dependencies, normal Tauri build prerequisites, libmpv/GTK/OpenGL stack, ffmpeg, and ffprobe.
2. Install `tauri-driver`:

   ```bash
   cargo install tauri-driver --locked
   ```

3. Install WebKitWebDriver, or set `WEBKIT_WEBDRIVER_PATH` to its full path.
4. If `tauri-driver` is not at `~/.cargo/bin/tauri-driver`, set `TAURI_DRIVER_PATH` to its full path.
5. Put ffmpeg on `PATH` or set `FFMPEG_PATH` so the mixed-media workflow can generate temporary GIF and MP4 fixtures.
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
| Product/window title | `Image Viewer 3000 E2E` |
| Cargo target directory | `src-tauri/target-e2e` |
| Executable | `src-tauri/target-e2e/debug/media_tagger` |
| Build kind | Debug, unbundled (`--debug --no-bundle`) |
| WebDriver endpoint | `127.0.0.1:4444` |
| Production identifier kept separate | `com.example.mediatagger` |

Tauri derives the E2E app-data directory from `${XDG_DATA_HOME:-~/.local/share}/com.example.mediatagger.e2e`.

### Isolation guarantees and cleanup guards

The build helper refuses to build unless all of these conditions hold:

- the overlay identifier is exactly `com.example.mediatagger.e2e` and ends in `.e2e`;
- the configured window list contains the exact title `Image Viewer 3000 E2E`;
- the resolved target directory is not `src-tauri/target` and ends in `target-e2e`.

The pre-run app-data cleanup resolves both paths and refuses recursive deletion unless:

- the E2E and production app-data paths differ;
- the E2E directory's parent is exactly the resolved app-data root;
- its basename is exactly `com.example.mediatagger.e2e`;
- the identifier ends in `.e2e`.

Only then does it recursively remove the E2E directory with retries. An `EPERM` lock is a limitation: the runner warns, skips that cleanup, and continues, so stale isolated E2E state may remain. The live-title check is the final guard before specs containing destructive actions.

### Fixtures, selectors, and destructive workflows

`app.smoke.e2e.js` indexes the checked-in PNG assets under `src-tauri/icons` and exercises navigation, settings, bulk tags, and bulk groups. It does not delete those source files. `app.workflows.e2e.js` creates unique roots with `fs.mkdtemp`, uses `e2e/fixtures.js` to copy valid PNG icons or generate a small GIF and two MP4 files with ffmpeg, and records temporary CSV/ZIP artifact paths under the operating-system temp directory. Its `afterEach` clears the isolated library database and removes only those roots and artifacts. Video coverage includes a native IPC scenario for paused seeking, fullscreen session continuity, retained preferences, autoplay on replacement, and stale closes. These checks use backend snapshot attributes, not GTK button clicks or video pixels. Other video coverage checks authorized native opening, the full native-player footprint, absence of a DOM control rail, rapid source switching, absence of an HTML `<video>`, error fallback, and native-surface cleanup. WebDriver cannot inspect pixels or controls rendered by GTK above the WebView.

The workflow suite deliberately tests library clear, scan-root removal, backup restore, and permanent media deletion. Keep every deletable media fixture under a newly created temp root. Never change a destructive spec to index a personal directory, the repository root, or production app data. Do not weaken the identifier, target-directory, app-data-path, or live-title guards to make a failing run proceed.

Desktop selectors currently mix stable IDs/classes/data attributes, `aria-label` values, visible English text selectors such as `button=Apply`, and a small number of XPath expressions. Prefer stable accessible roles/names or explicit test IDs when text is not itself the behavior. If visible copy or accessibility labels change, update the associated E2E selectors in the same change.

The desktop harness does not force English. A fresh app chooses a stored language first and otherwise follows `navigator.language`, while the current specs assert English labels. Because pre-run cleanup removes the isolated profile, a manually persisted selection is not a durable fix; run with a browser environment that reports English or update the harness so language setup is deterministic. Vitest does force English through `src/test/setup.ts`; that does not affect the desktop process.

### UI redesign matrix

`e2e/specs/ui.redesign.e2e.js` is opt-in with `MEDIATAGGER_UI_REDESIGN=1`. It uses fresh
copied media and the normal isolated E2E identity. It exercises both themes at 1440×900 and
1000×720, search/suggestions, validation and empty results, bulk selection, tag browsing,
lightbox deletion cancellation, settings navigation, duplicates and confirmation dialogs.
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
the shared GIF/two MP4 fixtures, and seeds only that temporary media directory. It sets
English in this isolated WebView before reloading. Existing dev/release profiles and the
ordinary E2E runner's cleanup behavior are unchanged.

Evidence is retained under ignored `artifacts/app-control/<session>/`: PNG screenshots,
`actions.jsonl`, `console.jsonl`, `process.log`, and `session.json`. `logs` returns the latest
24,000 characters from each log. Frontend console collection attaches after connection,
preserves original console behavior, and records reload gaps or buffer overflow. It does
not capture the earliest frontend startup messages or worker consoles. Driver and inherited
application stdout/stderr are recorded from launch. `diagnose` emits known test messages;
it does not prove application error handling.

Screenshots capture the private display, including GTK overlays. Gallery/image screenshots
were visually verified. On the verification host, the native MP4 picture was black or
corrupt on Xvfb, including with software GL; decoded video pixels remain unverified.
WebDriver selectors cannot control native GTK playback buttons or native file dialogs.
Do not change playback code or weaken isolation to work around these limitations.

`stop` and handled signals close the WebDriver session and owned process groups before
removing the marked temporary root. Stop is repeatable, and evidence survives. Failed
startup uses the same cleanup. If owned processes cannot be stopped, temporary data stays
in place. SIGKILL/host crashes can leave a lock and temporary state; inspect recorded
process identities before manual recovery, never kill by process name or automatically
remove a lock merely because a connection failed.

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
