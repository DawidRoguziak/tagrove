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
| `bun run test:e2e-build` | Checks frontend build order, profile rejection, preference isolation, and WDIO failure exit with fake build tools in a disposable project, plus the native-screenshot display guard (`e2e/display.test.js`) against fake process tables. |
| `bun run test:app-control` | Runs Node tests for controller input validation, session-state gating, probe-timeout classification, timeout budgets, the command queue (stop outside the queue, skipped stale commands, coalesced console polls), local-port conflicts, temporary-directory ownership, process-tree cleanup, and shared PNG/GIF/MP4 fixtures. Checks both playable MP4s with ffprobe for video and zero audio streams, and the audio video fixture for one video stream, one audio stream, and the requested length. Requires Linux local sockets, ffmpeg, and ffprobe. |
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
| `bun run test:client` | Runs the locked tests of `media_tagger_client` in the GPUI workspace (`src-gpui/Cargo.toml`), including the `Backend` port on the real core with temporary profiles. No GUI stack is compiled for it. |
| `bun run test:gpui` | Runs the locked tests of `media_tagger_gpui`: unit tests and the integration targets `controls`, `gallery_grid`, `gallery_position`, `gallery_position_render`, `gallery_selection`, `kit`, `overlays`, `scenario`, `scenario_window`, `search_ported`, `search_window`, `shell_ported`, `shell_window`, `startup`, `tag_editor_window`, `tag_list_window`, `text_input` and `text_input_owner`. Needs no display; see [GPUI workspace](#gpui-workspace). |
| `bun run gpui:fmt` | Runs `cargo fmt --all -- --check` for the GPUI workspace. |
| `bun run gpui:clippy` | Runs locked Clippy for all targets/features of the GPUI workspace, denying warnings. |
| `bun run gpui:lock-parity` | Runs `scripts/gpui-lock-parity.py`: the core's dependencies are identical in `src-tauri/Cargo.lock` and `src-gpui/Cargo.lock`, and the GPUI lock has only reviewed sources. |
| `bun run test:gpui-tools` | Runs the tests of the lock guard (`scripts/test_gpui_lock_parity.py`, synthetic locks plus the repository's own) and of the GPUI E2E build script (`scripts/gpui-build-e2e.test.mjs`, with a fake `cargo` in a disposable project). |
| `bun run audit:gpui` | Runs Cargo audit against `src-gpui/Cargo.lock` with no advisory exclusions. Requires `cargo-audit`. |
| `bun run gpui:control` | Keeps the GPUI E2E binary open on a private Xvfb display with scratch XDG directories: named start states and screenshots, no input. See [comparison harness](#comparison-harness-tauri-and-gpui). |
| `bun run compare:ui` | Captures the named scenarios in the Tauri application (through `app:control`) and in the GPUI application (through `gpui:control`) and writes both screenshots, a side-by-side image and `result.json` to `artifacts/gpui-compare/<scenario>/`. Display-bound; never next to another desktop run. |
| `bun run test:compare` | Runs `node --test e2e/compare`: the guards, catalogue and result tests of the comparison harness. Needs ffmpeg and no display. |
| `bun run quality` | Runs locale validation/tests, application and Vite/Vitest type checks, Biome lint/format checks, rustfmt, locked Clippy with warnings denied, production Bun audit, and RustSec audit with the explicit advisory exclusions in `audit:rust`; then the GPUI workspace part: `gpui:fmt`, `gpui:clippy`, `gpui:lock-parity`, `test:gpui-tools`, `test:compare` and `audit:gpui`. |
| `bun run test:all` | Sequentially runs `quality`, frontend Vitest, the complete locked Rust test suite, the GPUI workspace tests (`test:client`, `test:gpui`), and desktop E2E. It stops at the first failed layer. |

`bun run tauri:build:e2e` is a test-support script rather than a test runner. It invokes `e2e/build-e2e.js` to create the isolated unbundled debug executable. It runs `bun run build` for the application before invoking Tauri. Its overlay disables the redundant `beforeBuildCommand`.

Focused examples:

```bash
bun run test -- src/features/lightbox/__tests__/LightboxModal.test.tsx
bun run test -- --maxWorkers=1
cargo test --manifest-path src-tauri/Cargo.toml --locked module_or_test_name
cargo test --manifest-path src-tauri/Cargo.toml --locked --test backend_integration test_name
cargo test --manifest-path src-gpui/Cargo.toml --locked -p media_tagger_client test_name
cargo test --manifest-path src-gpui/Cargo.toml --locked -p media_tagger_client --test text_editor
cargo test --manifest-path src-gpui/Cargo.toml --locked -p media_tagger_gpui --test shell_window
cargo test --manifest-path src-gpui/Cargo.toml --locked -p media_tagger_gpui --test startup
cargo test --manifest-path src-gpui/Cargo.toml --locked -p media_tagger_gpui --test scenario
cargo test --manifest-path src-gpui/Cargo.toml --locked -p media_tagger_gpui --test scenario_window
cargo test --manifest-path src-gpui/Cargo.toml --locked -p media_tagger_gpui --test text_input
cargo test --manifest-path src-gpui/Cargo.toml --locked -p media_tagger_gpui --test text_input_owner
cargo test --manifest-path src-gpui/Cargo.toml --locked -p media_tagger_gpui --test overlays
cargo test --manifest-path src-gpui/Cargo.toml --locked -p media_tagger_gpui --test controls
cargo test --manifest-path src-gpui/Cargo.toml --locked -p media_tagger_gpui --test kit
cargo test --manifest-path src-gpui/Cargo.toml --locked -p media_tagger_gpui --test gallery_grid
# a measurement, not a check: a frame while scrolling through 100,000 synthetic results
cargo test --manifest-path src-gpui/Cargo.toml --locked --release -p media_tagger_gpui --test gallery_grid frame_time -- --ignored --nocapture
# a measurement, not a check: a frame of the tag list with 1,000, 5,000 and 20,000 loaded tags
cargo test --manifest-path src-gpui/Cargo.toml --locked --release -p media_tagger_gpui --test tag_list_window frame_time -- --ignored --nocapture
node --test e2e/compare/scenarios.test.js
```

There is no numeric coverage threshold. Run the frontend and backend suites explicitly; they are not CI merge gates.

Solid library model tests live beside `src/library` owners and run with `bun run test -- src/library`.
They use Solid Testing Library's owned `renderHook` and `src/test/settle.ts` for explicit
batch boundaries. Deferred promises control races; queue tests use fake timers. The model
integration suites cover mutation admission/draining, canonical publication, query and
identity invalidation and cache bounds. They mock platform
IPC and do not establish native-bridge or desktop feature parity.

The opt-in `e2e/specs/search-recovery.e2e.js` suite runs with
`MEDIATAGGER_SEARCH_RECOVERY=1`. It uses temporary PNG fixtures and faults the first
suggestion-worker construction after each reload. Retry then runs the real bundled worker.
It checks native pointer and Tab/Enter recovery in search, lightbox and bulk editors,
plus literal `constructor` and `__proto__` selection through the rendered tag modal.
Its native clicks go through `e2e/native-input.py`, which needs `MEDIATAGGER_NATIVE_DISPLAY`
equal to `DISPLAY` and a `/tmp/mediatagger-*` `XDG_DATA_HOME`; without the display variable the
first native step fails with a `KeyError`. Run it on a private display with disposable XDG
directories, using the isolation rules below:

```sh
dbus-run-session -- xvfb-run -a -s '-screen 0 1440x1100x24 -nolisten tcp' \
  sh -c 'export MEDIATAGGER_NATIVE_DISPLAY="$DISPLAY"; MEDIATAGGER_SEARCH_RECOVERY=1 exec bun run test:e2e:tauri --spec e2e/specs/search-recovery.e2e.js'
```

## Continuous integration

`.github/workflows/privacy.yml` is the only GitHub Actions workflow. It runs the `privacy` job on pull requests and pushes to `main`, using `ubuntu-24.04`. It fetches full history, installs checksum-pinned Gitleaks 8.30.1, scans publication data, and runs the privacy regression tests. It does not upload artifacts or use an Actions cache.

Frontend checks, Rust checks, application tests, and Linux packaging are run explicitly with the commands documented here and in [Linux packaging](linux-packaging.md). The local `audit:rust` script excludes `RUSTSEC-2026-0194` and `RUSTSEC-2026-0195` without recording a rationale; an excluded advisory is not proof of safety. `audit:gpui` has no exclusions: `src-gpui/Cargo.lock` pins the `quick-xml` users of the GPUI tree at releases that fix both advisories, and it reports the remaining unmaintained and unsound notices as warnings. Real desktop E2E uses the isolated `.e2e` profile on a configured Linux host. The Docker release build does not run tests.

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

Binary IPC responses (`tauri::ipc::Response`, currently only
[`get_asset_query_id_range`](../architecture/ipc-contract.md#binary-id-ranges)) reach
JavaScript as an `ArrayBuffer`, not JSON. Mock them at the layer under test with
`src/test/idRange.ts`:

- when a test mocks `@tauri-apps/api/core` `invoke` (wrapper tests in
  `src/platform/tauri/__tests__/api.test.ts`), resolve `idRangeBuffer(ids)`, the real
  f64 little-endian `[count, ...ids]` body; resolve an empty `ArrayBuffer` for `stale`;
- when a test mocks the `platform/tauri/assets` module (model and component tests),
  mock `getAssetQueryIdRange` and resolve `idRangeResult(ids)` (a decoded
  `{ status: "ready", ids: Float64Array }`) or `{ status: "stale" }`;
  `sequentialIds(start, end)` builds the usual index + 1 fixture IDs.

A decoded mock does not exercise `decodeAssetIdRange`; keep its edge cases (unaligned
views, malformed lengths, count mismatch) in the wrapper tests.

The setup awaits `i18n.changeLanguage("en")`. Tests that change language or rely on persisted language must restore their state; the setup is not rerun before every individual test.

## Rust unit and integration tests

Rust unit tests live in `#[cfg(test)]` modules beside their implementation. This is the right layer for private helpers, parsing and normalization, database query semantics, validation, scheduler behavior, and service/command branches that do not require a running Tauri application.

The framework-free startup phases are covered the same way: `src-tauri/crates/media_tagger_core/src/app/bootstrap.rs` runs `open_storage` and `start_workers` on a temporary profile (interrupted restore rolled back before the database opens, abandoned backup temporaries swept, an uncommitted file operation recovered and the startup connection left FULL-sync (the order of those two and of the snapshots is not pinned), the scan-root snapshot, a corrupt or newer database refused and left unchanged, `AppState` built) and checks ffmpeg search precedence. The adapter side in `lib.rs` (identifier guard, `InstanceLock`, video phase) has no Rust test; only the position of `manage(AppState)` as the last step of `setup`, which ends the `thumb://` 503 window, is checked statically by `app_state_is_managed_last_in_setup` in `architecture_boundaries`. Desktop E2E (`app.smoke`, `startup-scan`, `thumbnail-fade`) and a `bun run tauri:dev` launch prove the running sequence. The startup refusal when a thumbnail worker fails to spawn (`worker_spawn_failures` in `start_workers`) has no test, because a failed thread spawn cannot be provoked without a port.

The backend is a workspace of two packages, the adapter `media_tagger` and the core `media_tagger_core` (see [system overview](../architecture/system-overview.md#backend-module-boundaries)). `bun run test:backend` runs the unit tests of both and every integration target; narrow it with `cargo test --manifest-path src-tauri/Cargo.toml --locked -p media_tagger_core` (core unit tests, no GUI stack) or `-p media_tagger --lib` (adapter unit tests). `--test <target>` needs no `-p`, because only the adapter has integration targets. The adapter's player tests use the core's ADR 0028 fixtures (`test_support.rs`) through the core feature `test-support`, which only the adapter's `[dev-dependencies]` enable.

Cargo discovers integration targets in `src-tauri/tests/`. Inspect the current target list with `cargo metadata --manifest-path src-tauri/Cargo.toml --locked --offline --no-deps --format-version 1`, selecting targets whose `kind` contains `test`. The focused package scripts select only `backend_integration` and `backend_e2e`; they do not cover every integration target.

The integration binaries belong to the adapter package and use the public `media_tagger_core` API (`architecture_boundaries.rs` also imports the command and video DTOs from `media_tagger`). Examples include:

- `backend_integration.rs` covers file-backed asset/tag queries, root-prefix deletion and distinct thumbnail paths, complete library clearing, bulk tag merge, bulk media-group replacement, and file-mutation journal recovery across a restart (root removed, library cleared, root unreachable, interrupted delete, and a bundle restored over a library with a committed delete awaiting cleanup).
- `backend_e2e.rs` composes database and tag helpers into one backend workflow: export-shaped rows, CSV-style tag parsing/normalization/merge, filtering, group replacement, and library clearing. It does not invoke the CSV command parser itself.
- `architecture_boundaries.rs` also scans the GPUI workspace `../src-gpui/crates` ([ADR 0036](../adr/0036-gpui-second-adapter-in-its-own-workspace.md)): `client_manifest_has_no_gui_dependencies` (the `GUI_CRATES` list plus `gpui*`, `wgpu*`, `blade*`), `client_reaches_the_core_only_through_its_backend_adapter` (outside `backend/core_backend.rs` only `media_tagger_core::{models, error}`; an alias, a glob import or a bare `use media_tagger_core` counts as a violation), `gpui_adapter_reaches_the_core_only_in_startup_composition` (outside `boot.rs` the same two modules and no use case; nowhere a `db::` path, lock helper, `Connection` or `admit`) and `gpui_port_dependency_direction`. `gpui_port_rules_flag_core_access_outside_the_allowed_files` and `scanner_finds_external_crate_modules` test the rules on synthetic sources.
- `architecture_boundaries.rs` guards what the compiler does not ([ADR 0034](../adr/0034-tauri-independent-backend-core.md)) and freezes the IPC wire contract (commands, wire arguments, DTO JSON shapes, event names). `core_manifest_has_no_gui_dependencies` parses `crates/media_tagger_core/Cargo.toml` (std only, with a self-test on a synthetic manifest) and rejects any Tauri, GTK, WebKit or libmpv crate. A std-only tokenizer (comments and literals removed) scans both `src-tauri/src` and `src-tauri/crates/media_tagger_core/src`, telling the packages apart by root: the core's `db` does not reach `services`, `app` or `usecases`; the adapter's `video/` does not import `commands/`; `commands_do_not_touch_db_or_locks` rejects a `db::` path, `Connection`, `admit`, a `with_*lock` helper and any `locks` path in the adapter's `commands/`, so SQL and workflow locks are reached only through the core's `usecases/`; the core's `app/bootstrap.rs` does not reach `usecases` (`bootstrap_does_not_import_usecases`); and `app_state_is_managed_last_in_setup` requires `app.manage(bootstrap::start_workers(..)?)` to be the last step of `setup` in the adapter's `lib.rs` (the `thumb://` 503 window), with a self-test on synthetic sources. Tests of that logic therefore live in `src-tauri/crates/media_tagger_core/src/usecases/*`, not beside the handlers. Run it with `cargo test --manifest-path src-tauri/Cargo.toml --locked --test architecture_boundaries`.

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

Two more real-libmpv tests build the player from the production option table with test overrides for the outputs. One checks that libmpv accepts the options and reports `video-timing-offset=0` and `audio-fallback-to-null=yes`. The other plays a generated MP4 with a silent audio track while the only allowed audio driver cannot open (`ao=pcm` writing into a missing directory), and requires advancing playback, `current-ao=null`, a selected audio track, and no error event. Unit tests in `video/surface.rs` check that a frame render passes `MPV_RENDER_PARAM_BLOCK_FOR_TARGET_TIME=0`, and run a private GLib main context in which a redraw is requested for every frame: a source queued at `DEFAULT_IDLE`, as wry queues IPC responses, must still run within one frame. None of these tests creates a GTK widget or an OpenGL context; the real render path is covered only by desktop E2E.

Reference blocking ([ADR 0028](../adr/0028-media-tools-open-only-the-file-they-are-given.md)) has real-tool tests on both sides. `reference_fixtures` in `test_support.rs` (core test fixtures shared by the player tests in `video/player.rs` and by `thumbs.rs`) writes eleven text files named `*.mp4` (m3u, pls, HLS, EDL, DASH, `concat`, SDP) that point at a `127.0.0.1` listener bound to a free port, at a generated clip outside the fixture folder and at a sibling clip. The player test opens each through the worker with the production option table and requires an error event, no `metadata` event, no connection and no open of a named clip, then plays a generated clip on the same player. Opens are observed with an inotify watch (`OpenWatch`), because access times are not recorded on a `noatime` temp filesystem. A companion test sets `access-references=yes` and requires the playlist to reach the listener and play the outside clip, which shows that the fixtures and both detectors work. `video_tools_refuse_reference_content_and_still_read_real_containers` in `thumbs.rs` runs the same files through `probe_video_duration_ms` and `create_video_thumb` with the system `/usr/bin/ffmpeg` and `ffprobe`, then checks MP4, MKV, MOV and AVI clips. The listener never answers and is the only network endpoint these tests name.

The same worker regression runs 30 cycles of all six speed choices, alternating same-file reopening and two-file replacement. It checks autoplay from the beginning, advancing time, retained rate and rejection of stale controls, close and failure events. Real-libmpv flag tests cover true/false values, property errors and guarded C-integer storage. Run these with `cargo test --manifest-path src-tauri/Cargo.toml video_ --lib`. The native lifecycle suites live in `src/features/lightbox/video/__tests__`, covering session races, labels, bounds acknowledgment, fullscreen, retry, keyboard ownership and disposal. The native bounds helper and its tests live beside the video model.

## GPUI workspace

The GPUI port ([ADR 0036](../adr/0036-gpui-second-adapter-in-its-own-workspace.md)) is tested in its own workspace; every command takes `--manifest-path src-gpui/Cargo.toml --locked`. None of these tests needs a display server, and none opens a real app-data directory: startup tests pass temporary directories to `boot::start` or a temporary `XDG_DATA_HOME` to `run_with`, the display check is tested against sockets in temporary directories, backend tests run the core on a temporary profile, and the profile tests are pure.

The client crate has three test doubles behind its feature `test-support` ([ADR 0037](../adr/0037-gpui-client-backend-port-and-model-convention.md)), which the crate's own tests get through a dev-dependency on itself and which a normal build never enables:

- `FakeBackend` (`backend/fake.rs`): script a method with `on_<method>(handler)`, read `calls()`, answer held thumbnail requests with `answer_thumbnail(index, reply)`, publish progress with `emit_progress`.
- `ManualExecutor` (`exec/manual.rs`): each backend call of a model is a waiting task; `run_nth(index)` decides which call answers first, `advance(duration)` moves the virtual clock for timers. A model test drives `Driver::handle`, runs tasks, then `Driver::pump`.
- `TempCore` (`backend/core_backend.rs`, module `test_support`): the real core opened with `bootstrap::open_storage` and `start_workers` in a fresh temporary directory with `data/` and `media/`; `write_image` adds a synthetic PNG, `relaunch` opens the profile again with a new startup-scan snapshot, and dropping it removes the directory. It has no ffmpeg, so video thumbnails are not covered there.

| Where | What it proves |
| --- | --- |
| `media_tagger_client` unit tests (`i18n.rs`, `i18n/`, `theme.rs`, `theme/`, `prefs.rs`, `prefs/`) | i18n: every English key resolves in every language to that language's own non-blank text, a missing key falls back to English and then to the key, plural rules of all eleven languages match `Intl.PluralRules` vectors, `formattedCount` matches `Intl.NumberFormat`, interpolation, `translate_backend_text` (the cases of `backendText.test.ts`), the language list, labels and locale files agree with `languages.ts` and `core.ts`, and the language is chosen from the preference, then `LC_ALL`/`LC_MESSAGES`/`LANG`, then English. Theme: the token constants match `src/styles.css` (parsed, with `var()` and `color-mix()` resolved), and a colour, shadow, motion or type-scale change on one side only, or a new stylesheet token, fails. Preferences: the file round-trips, a damaged or unreadable file reads as defaults, a failed save keeps the old file, concurrent saves leave one complete file. |
| `media_tagger_client::runtime`, `shell`, `window_chrome`, `shortcuts` | The application runtime with `FakeBackend`, `ManualExecutor` and `MemoryPrefsStore` (ports of `startupScanRemount.test.ts` and the runtime parts of `shell.test.tsx`): the start sequence runs once and stores the start theme and language; a startup scan without work refreshes nothing, one with work or a failure refreshes the library and the roots and finishes only after both answered, and a failure is kept, not retried, and blocks nothing; a late library or root answer changes nothing and a failed refresh keeps what was published; a library refresh reads the known tags with the query, two refreshes share one pass over them, and the startup scan finishes only after the query, the known tags and the roots answered; the embedded query model loads the page of a viewport, answers a ticketed read and runs its stale-refresh timer through `RuntimeInput::Query`; the embedded library model loads details, queues a favorite behind a tag edit and answers both through `RuntimeInput::Library`, and a query refresh sent as `RuntimeInput::Query` empties its details cache; views switch without a backend call and Back ends the scan highlight; theme and language change at once and are saved one save at a time, a failed save keeps the change; the tile size follows the slider and the wheel inside its range. The shell's empty state from roots and total, the item count with plural and digit grouping, every shell text in English. The window decorations: the maximise button follows the window state, grips exist only for a normal window and match the `.window-resize-edges` rules of `src/styles.css` (the test reads them), close turns red only under the pointer, a press and a move start one window move and a double click maximises. The shortcut registry: contexts nest inside `Shell`, no keystroke is bound twice in one context (the gallery's six scroll keys are its first entries), and the keys that type a character are recognised. The embedded selection (epic E11, the `selection` cases of `runtime/tests.rs`): a Shift range is read through the query model's `get_asset_query_id_range` and published from its answer, which never shows up in `take_query_events` while another caller's ticketed read still does; a failed range read sets `gallery_load_failed` and Retry reads it again; a refresh that overtakes a range read leaves neither a selection nor an error; IDs survive a new session with their `key`, ranges survive a refresh into the same session, are cleared with `RangeCleared` by another session and with `RangeExpired` by a failed query; a held range reads one ID of its session at once and then every 60 s of the executor's clock, and stops when it is cleared; a range whose first keepalive read is `Stale` is dropped with `RangeExpired`; a range selection is written through `apply_selection_mutation` with its spec, and a `Stale` write refreshes the query and clears it; an identity reset clears the selection and keeps selection mode. |
| `media_tagger_client`, `tests/app_runtime.rs` | The runtime through the public API of the client crate (ports of `startupScanRemount.test.ts`, `shell.test.tsx`, `preferenceStorage.test.tsx` and the shell part of `gallery.test.tsx`). With `FakeBackend` and `ManualExecutor`: a startup scan with work publishes its rows for 0, 3 and 160 initial rows and is never asked again, a launch without startup work refreshes nothing more, a failed scan publishes what it committed and is not retried; the first-folder route highlights scanning until Back; every language is live and stored; an unknown stored theme starts dark; the count shows the total and a settings round trip queries nothing; the runtime state survives view switches. On the real core through `TempCore` with a `FilePrefsStore` in the temporary profile: a second runtime of the same launch replays the startup scan instead of scanning again (single flight); an empty profile starts into the "Add first folder" state; theme and language survive the view round trip and the next launch; a preference path that can be neither read nor written starts on the defaults and still changes for the session; and the runtime starts on the production `ThreadExecutor`, scans once and publishes the library and roots. |
| `media_tagger_client`, `tests/search_ported.rs` | The search behaviour of the Solid specification (epic E12) through the public API of `media_tagger_client::search`: the grammar cases of `media.test.ts` and `literalTagSearch.test.ts` (operators, counts up to `Number.MAX_SAFE_INTEGER`, quoted literals round-tripping through typed search and suggestion replacement), `tokenService` and `inputService` (the token under the caret, byte offsets), `filterService` and `createSearch` (draft and applied state, an unchanged resubmit, a refused submit keeps the applied filters until Clear), the parser errors with their own text in every language, and the suggestion session cases of `createSuggestions` and `SearchTagator` (a late answer to an older draft is dropped, a group name asks for nothing). With `FakeBackend` and `ManualExecutor`: every valid command starts one query with the whole draft, an unchanged known-tags refresh builds the suggestion index once, and a request for 100,000 tags only queues work. On the real core through `TempCore`: tag, kind, favourites, `tags`, `tags:N`, `gN:` and literal filters count the matching assets (the `search-applied` state). |
| `media_tagger_client::search::tag_list` (`search/tag_list.rs` with `tag_list/tests.rs`, `selection.rs`, `layout.rs`) and its runtime cases in `runtime/tests.rs` | The tag list model (epic E13; ports of `createTagList.test.ts`, `tagListSelectionService.test.ts` and the model cases of `TagListModal.test.tsx`) with answers given by hand: a page of an older filter or of a closed list is dropped, one page request runs at a time and closing stops it; a failed next page is filled from the known tags at its offset, and when that adds nothing loading on its own stops until a scroll asks again; the known-tags fallback is prepared only while the list is open, deduplicated keeping the first spelling and filtered, and a new vocabulary while open starts over; whitespace around the filter starts nothing; the placeholder tells loading, an empty library and no match apart; only a scroll within 120 px of the end or content no more than 120 px taller than the list loads the next page, and a scroll back to the end after leaving it loads again; a click includes only after the 220 ms timer, a click on another tag applies the waiting one, a stale timer does nothing and a double click cancels the waiting click; a single click includes an inactive tag and clears a chosen one, a double click toggles exclusion, and the choices become a filter sorted without case and Latin diacritics; a failed Apply keeps the list and its choices; literal names pass unchanged. `layout`: the classes of `TagListChipList.tsx` still match, each mode has its colours, the content fits the window less 2 rem. In the runtime with `FakeBackend` and `ManualExecutor`: pages of 100 through `list_tags` and the click timer on the executor, Apply writes `cat -dog` into the search and closes the list only after the query answered; a failed query keeps it open with its choices; a failed first page shows the library's known tags. |
| `media_tagger_client`, `tests/tag_list_core.rs` | The tag list as a workflow over the runtime (epic E13). On the real core through `TempCore` in a temporary library: the first page holds 100 of 152 tags, the next page the rest with no tag twice, and a filter starts over with the matching tags; `camera` included and `cat` excluded search SQLite, find the one matching image and close the list once the query answered. On `FakeBackend`: Apply without choices clears the search and closes the list, Apply with unchanged choices closes it again, and a page of an earlier opening is dropped. |
| `media_tagger_client::tagging` | The shared tag editor without a GUI (epic E13): the classes copied from `TagEditor.tsx` still match the Solid source, the frame shows loading before the tags and the tags before the empty text, the two variants differ in height and background, and the suggestion list keeps to the window (`list_placement`). |
| `media_tagger_client::text` (`text.rs`, `text/`) | The editing model of the text field ([ADR 0038](../adr/0038-gpui-text-field-own-element-and-key-context.md)). Boundaries: the caret steps over whole grapheme clusters (ZWJ families, flags, skin tones, keycaps, combining marks, Hangul jamo, CJK), word steps skip whitespace and ASCII punctuation, a double click takes the segment under the pointer. `LineEditor`: typing, deletion by cluster and by word, selections made in both directions and across their anchor, replacement of a selection, clipboard cut and paste, a range replaced for a suggestion, undo and redo with their grouping and the 100-step bound, the acceptance filter, and offsets from outside moved to a character boundary. Pasted line breaks become spaces and trailing ones are dropped; a typed line break or tab never reaches the buffer. Input method: composing, committing (one undo step back to the text before the composition), an emptied composition, unmarking, a dead-key sequence, a rejected result, caret movement that leaves a composition alone, and every UTF-16 range up to two units past the end without a slice inside a character. Also the UTF-8 ↔ UTF-16 conversions, the key table (each keystroke bound once), the style runs (lengths always add up to the line), the horizontal scroll (the caret stays visible, no empty space behind the line), and the field's visuals against the `input` rules of `src/styles.css`. |
| `media_tagger_client`, `tests/text_editor.rs` | The same model through the public API of `media_tagger_client::text` only, as an owner uses it. Ported from the Tauri interface: a picked suggestion replaces its token (first, negative, with a multibyte tag, with the selection elsewhere) as one undo step and the caret lands behind it on a character boundary (`inputService.test.ts`); the confirmation field hands over case and surrounding spaces as typed and drops the line break a pasted word ends in; a paste the order field's filter refuses leaves text and selection alone. From the acceptance criteria, over texts with ZWJ sequences, flags, skin tones, combining marks, Hangul jamo and CJK: no motion leaves the caret or the anchor inside a cluster, no deletion removes part of one and undo restores it, selecting cluster by cluster to the left takes the whole text as a reversed selection, a selection grown one way shrinks back the other way, every cluster boundary survives the round trip through UTF-16, a multi-line paste is one line and one undo step, and undo brings back a reversed selection that typing replaced. |
| `media_tagger_client::backend` (`backend.rs`, `error.rs`, `id_range.rs`) | A thumbnail request needs a positive ID and a 16-hex version token; a responder answers once and a dropped one answers `Unavailable`; `i18n:<key>` texts name their locale key and other core text stays unchanged; the binary ID range decodes to typed IDs, with an empty body as `Stale` and malformed or incomplete bodies refused. |
| `media_tagger_client::backend::core_tests` | Every method of the `Backend` port on the real core, through `TempCore` (a temporary profile and media directory with synthetic PNGs): query sessions (filters, anchor, superseded generation, validation texts), pages, positions, typed ID ranges, sections, summaries, details, the tag list, tag, favorite and media-group writes, selection mutations (also stale), delete, duplicate search and resolution, scan roots and scans with progress, the startup scan (launch snapshot, one scan for concurrent callers, replay), tool status, bulk render, retry, cancel and clear, on-demand thumbnails (render, serve, stale version, cached-only miss, broken source, malformed request, 24 at once), CSV export and import, clearing the library, and a bundle export, inspection and restore. Also: a backend without its core answers `Unavailable` and `NotReady`, a tile requested during startup gets its picture after `attach`, no workflow lock outlives its call, and calls from many executor threads all finish. Destructive calls only ever see the test's own temporary directory. |
| `media_tagger_client::backend::fake` | `FakeBackend` records calls with their arguments, answers through per-method handlers, fails an unscripted call with its name, holds thumbnail requests until the test answers them in any order, and emits progress to subscribers. |
| `media_tagger_client::model` | The convention on a small model: state changes only after the backend answered; a late reply with a stale token changes nothing, whether it arrives after or before the current one; a failed call keeps the published state; timers are effects and an obsolete one is ignored; a panicking call and a task the executor dropped still answer with `WorkerFailed`; the same model runs on `ThreadExecutor`, off the model thread, and wakes its owner once per burst. |
| `media_tagger_client::exec` | `ThreadExecutor`: tasks run off the calling thread, a long task does not hold back the tasks behind it, the pool respects its limit, idle workers end, a panic leaves the pool usable, timers fire in order and not early, shutdown drops queued tasks and timers. `ManualExecutor`: nothing runs until the test runs it, a later task can run first, timers follow the virtual clock. |
| `media_tagger_client::progress`, `startup_scan` | Listeners get events only while subscribed, in order, and may unsubscribe during delivery; forwarded progress arrives as model inputs. The startup scan hands its roots out once under contention, never scans without an enabled root, lets observers wait for the running scan and replays its result, and retains failures and worker panics without a retry. |
| `media_tagger_client::library` (`library/query.rs` with `query/tests.rs`, `page_cache.rs`, `filters.rs`, `query_impact.rs`) | The query model with `FakeBackend` and `ManualExecutor` (ports of `createAssetQuery.test.ts`, `queryRaces.test.ts` and the query cases of `createLibrary.test.ts`): only a `Ready` start of the current generation replaces the cache, whichever of two starts answers first; a late page reply of the previous generation never merges and cannot remove the replacement request of its offset; pages answered in another order land at their own offsets; a `Stale` page while other pages load schedules one refresh and the others still merge; stale refreshes are debounced (250 ms), at least 1 s apart and cancelled by an explicit refresh, on the virtual clock; a failed start leaves no session and Retry recovers, a current `Superseded` keeps the rows on screen, a failed page is not requested again until Retry and `RetryAsset` unblocks one page; readers of one page share one request; the cache holds at most 24 pages, also for an oversized range and a million results; render-only fallback rows and their end; the anchor of a refresh and the paused viewport; ID ranges in one call, in chunks, whole or an error, and cancelled by refresh, reset and disposal; the keepalive read; sections once per session and ADR 0027; positions; a patch that crosses a reply makes the model read the page again; reset and disposal. `PageCache` alone: index order, eviction order, the protected range, fallback, patches, and no structure growing with `total`. The query-impact rule (`none`, `tags`, `all`, favorites). Three cases run the model on `ThreadExecutor` against the real core through `TempCore`: a scanned library is read on background threads (the first page, the pages of the viewport, an ID range clamped to the result, a position, the keepalive and the timeline sections); after a scan that added files the next page read of the old session is `Stale`, and the model replaces the session after the debounce and publishes the new total; an anchored refresh under a paused viewport carries only the visible page, with the rows an unanchored read found at the same indexes. |
| `media_tagger_client::library` (`library/tag_state.rs` with `tag_state/tests.rs`, `vocabulary.rs` with `vocabulary/tests.rs`, `library_model.rs` with `library_model/tests.rs`, `mutations.rs`, `details.rs`, `tags.rs`) | The metadata models ([ADR 0041](../adr/0041-gpui-library-model-queued-writes-as-data.md); ports of `createAssetTagState.test.ts`, `createKnownTags.test.ts`, `mutations.test.ts` and the list cases of `media.test.ts`). `AssetTagState` alone: a pending write blocks detail reads and its canonical result is published normalized; writes on one asset run in admission order and a failed one releases the next; overlapping multi-asset writes follow admission order without deadlock; a read started during a write is obsolete also when the write failed; a barrier refuses new writes at once, waits for queued and running ones and is handed out once, barriers run one after the other; writes admitted before a reset still count and cannot start, and the new identity starts empty queues; a queued write for a deleted asset is refused; disposal rejects waiting barriers, queued writes and new admissions; a reused ID loads fresh details after a reset; tombstones; the 512-record bound with pins and held assets, and 1,200 assets of one write; the selection-wide turn. `KnownTags` with `FakeBackend` and `ManualExecutor`: pages of 200 until the total, a failed later page publishes nothing, a refresh during a pass joins it and the superseded read (failed or not) starts over, reset and disposal publish nothing and answer their readers at once, unchanged saves read nothing and keep the published `Arc`, lists compare by ordered values and length, a failed refresh is retried by the next save. `Library`: two quick tag edits of one asset are sent one after the other, the second computed from the first one's canonical result; a favorite behind a pending tag save; a failed write publishes nothing and the next starts from canonical tags; a tag edit is refused until the complete tag list is known; a bulk write waits for every asset, is sent once for all of them and publishes only acknowledged IDs (tags, favorites, groups); refresh rules under the filters applied at settlement; maintenance waits for admitted writes, refuses new ones and resets inside the barrier; a write of another identity or of a disposed owner publishes nothing; a deletion leaves a tombstone, refuses the writes queued behind it without a backend call and answers after its refreshes; details reads are shared, the cache keeps 256 records, and a read overtaken by a new query, a reset or the disposal, or crossed by a write, answers nothing; a restore keeps the known tags and a clear drops them; selection writes take the library-wide turn, drop every cached tag list, and a stale one refreshes the query. `tags.rs`: the U+FEFF vectors shared with `src/utils/__tests__/media.test.ts` and the core's `FEFF_VECTORS`, and `isValidTag`. One case runs the library on `ThreadExecutor` against the real core through `TempCore`: details, two quick edits of one asset, a bulk merge with a missing ID and a deletion in the temporary library. |
| `media_tagger_client::thumbs` | The thumbnail pipeline: retry delays (500 ms, 2 s, then 4 s doubling to a 30 s cap) and cycling retry numbers; `Failed` reported once after the quick retries; a cached-only miss and a malformed request are final; modes only get stronger and a visible tile drops its pending prefetch; cancelled, superseded and repeated replies and obsolete timers change nothing; a request cancelled before it started never reaches the backend; a tile asked for before the backend is ready shows its picture after the retry. `ThumbnailFetcher`: at most `MAX_CONCURRENT_FETCHES` executor tasks send requests and the rest wait in its queue; a request cancelled while it waits is not sent; a dropped lane and a panicking request do not strand the queue; requests blocked inside the backend leave `ThreadExecutor` threads to other tasks. |
| `media_tagger_client::gallery` (`gallery/geometry.rs`, `virtual_grid.rs`, `thumbnail_demand.rs`, `picture_cache.rs`, `layout.rs`) | The gallery grid without a GUI ([ADR 0042](../adr/0042-gpui-gallery-grid-logical-offset-and-bounded-pictures.md); ports of `gridGeometry.test.ts` and of the model cases of `createVirtualGrid.test.tsx`, without the WebKit window cases). Geometry: exact square rows at 10k, 26k and 1M slots, the last partial row, an offset beyond the end, the resize anchor at a million slots. `VirtualGrid`: the first layout starts at the top of the margin; the visible range names the pages to request at any depth (one page, a range across a page boundary, the last page of 1,000,000 results); a quarter-pixel step is exact at 600,000,000 px; scrolling is clamped to the margin and the footer; a short result does not scroll; a resize keeps the first visible index and a margin change keeps the scroll position; a shrinking result clamps; a half-viewport jump pauses until the idle timer of the last step fires and an obsolete timer ends nothing; slow scrolling stays active and distance adds up across direction changes within 100 ms; jumps and layout anchors are not motion; a hold pauses until release or rest. `ThumbnailDemand`: modes at first showing, the one promotion of a pending prefetch and none during a pause or after a failure, cached-only requests during a pause and their render after a miss, cancels for tiles that left the range, nothing for a cached picture, one decode per delivery, an undecodable answer. `PictureCache`: the cache never holds more than its budget over 1,000 pictures and over a 5,000-picture scroll at the default budget, the least recently used goes first, pinned pictures are never dropped even over the budget, a picture of another version lives only while pinned, replacing and clearing give every picture back. `layout`: six columns of 221 px in the default window, the banner's height above the grid, the footer's height, the chips of videos and GIFs, group backplates, the result count with plural and digit grouping, the placeholder pulse. |
| `media_tagger_client::gallery` (`gallery/selection.rs` with `selection/tests.rs`, `selection_geometry.rs`, `selection_gesture.rs`) | Bulk selection without a GUI (epic E11; development tests ahead of the ports of `createSelection.test.ts`, `selectionGeometry.test.ts` and `createGridHandlers.test.tsx`). `Selection` with reads answered by hand (asset ID = index + 1): clicks toggle at once and turning the mode off clears; a Shift range and a rectangle with gaps are one read of their envelope; a 100,000-position range and select-all of 1,000,000 read nothing, and 1,000,000 `is_selected` lookups stay cheap; Ctrl-clicks inside a range exclude and outside it include, and a larger range re-covers both; an incomplete or failed read keeps the selection and Retry repeats it once; a late answer after a query change, a clear, a disable, a disposal, an identity reset or a newer range changes nothing; exactly 10,000 positions become IDs that survive a new session, one more stays a range that another session clears with the notice; IDs keep their `key` and their `ids` list across a session change; an ID kept from an earlier query is found by the envelope read of a large range, or stays included outside it; the keepalive reads at once and once per interval only while ranges are held, never overlaps, retries after a failure, stops on clear, mode off, session change and disposal, and its first `Stale` answer drops the ranges while a later or a late one does not; a failed refresh drops ranges and keeps IDs; `spec_mutation` builds the three selection writes and answers `None` for IDs and while busy. Geometry: rectangles in any direction, tiles overlapped by a positive area only, rows joined only across every column, the short last row, empty rectangles and grids, global indexes far down 1,000,000 results, the edge-scroll speed, the tile under a point. `SelectionGesture`: a rectangle needs both the 150 ms hold and the 5 px move; the preview follows the pointer in any direction, clamped to the viewport and clipped when the pressed point scrolled away; the release commits the ranges, additive with Ctrl, and swallows one pointer click (never a keyboard one), which the next press forgets; a long press opens the pressed asset only (not after a move, a release, or when another asset sits at the index) and swallows its click; the edges scroll per frame for at most 32 ms; Escape, Ctrl+A, layout changes, losing the window and a change of mode or query epoch end what they must and leave a released read alone. |
| `media_tagger_client::bulk` (`bulk/tag_merge.rs`, `group_order.rs`, `selected_summaries.rs`, `dock.rs`, `board.rs`, `layout.rs`) | The bulk dock without a GUI (epic E14; development tests ahead of the ports of `src/features/bulk/__tests__`). A draft is one trimmed, lowercased tag or refused, and merged lists keep the first spelling of each tag; group keys are trimmed, assignments are one-based or cleared, a shared group is sorted by its saved order (assets without one after them, as selected), mixed groups conflict, a drop puts the dragged ID where its target is, and a typed order is a positive safe integer or empty. `SelectedSummaries`: cached rows need no read, the rest is read in batches of 256 with four reads out, omitted and failed records are failures until the next restart, an answer of a replaced selection or identity is dropped, a published write is laid over an older answer, and a deleted asset loses its record. The generated group key is a version 4 UUID; the tablist keys wrap and jump. `BoardLayout`: columns from the smallest tile, only the rows near the viewport, the drop on the tile under the pointer or the nearest one, the insertion line after a later and before an earlier target, the arrow keys by one tile or one row, and the least scroll that shows a moved row; a tile is its preview, the 44-pixel caption and the border high, which leaves 10 pixels of the row under it; the edge-scroll speed; `BoardDrag`: five pixels make a drag, a click, a release over nothing and a cancel drop nothing. The dock's measures never outgrow the viewport. |
| `tests/bulk_core.rs` (client) | The bulk model through `AppRuntime` on `ManualExecutor`. With `FakeBackend` (six images, IDs 1 to 6): one selected asset loads its complete tags and edits replace them on the canonical list (a tag it already has needs no write); unknown tags are never replaced and a failed read is retried on request; a partial merge publishes canonical tags only for the acknowledged assets and reports 2 of 3; a partial favorite toggle unselects the asset the backend did not acknowledge; a group write uses the given order, refuses an order that is not exactly the selection without a write, and reports a partial answer; feedback of a write whose selection changed is dropped. With the real core on a temporary profile (`TempCore`, four synthetic images): tags of one asset, an add-only merge, a favorite toggle and a group with an explicit order are read back from the core, and an empty key removes the group. |
| `tests/bulk_ported.rs` (client) | The ports of `createBulk.test.ts`, `bulkGroupOrderService.test.ts`, the board cases of `GroupOrderModal.test.tsx` and the dock-state cases of `BulkDock.test.tsx` (each test names its TS case). The bulk model through `AppRuntime` with `FakeBackend` on `ManualExecutor`, where a pending promise is a call the test has not run yet: complete tags, normalized additions and removals, writes queued behind another owner's write and behind each other, a tag and a favorite in click order, feedback of a retired selection or identity dropped, zero-result and partial answers (only acknowledged IDs published, for tags and groups), offscreen IDs and their summary reads (batches of 256, four in flight, omitted records retryable), a typed key kept while metadata arrives, favorites-only and tag filters refreshed, the maintenance barrier waiting for queued favorites, and range selections of 50,000 results written through one spec call each (stale fails and refreshes, an explicit order is refused). The group services with the TS values, the board's rows and the scroll after a key move, and the `Dock` over the real runtime: an invalid draft, the focus asked for after a save and not for a changed selection, the notice of a dropped range, the heart's states, disabled controls during a rectangle, the inline and larger drafts, conflicts, and the larger view's save, failure, partial, retired and invalidated cases; the tab returns to Tags when the selection becomes empty, and a tab picked while a notice keeps the dock open with nothing selected stays. |
| `media_tagger_client::scenario` | Scenario ids are matched whole, are unique and are safe as file names; every scenario is in `scenarios` of `docs/plans/gpui-port/epics.json` and has its Tauri steps in `e2e/compare/scenarios/<id>.js` with the same theme and seed; the three gallery scenarios use the control seed at the default, smallest and largest tile size; `gallery-selection` makes the clicks of its Tauri steps (same tiles, same modifiers, same order, only the last one with Shift), and `selection_inputs` waits for the session and the rows of the clicked tiles and clicks the assets those tiles show; the two `search-` scenarios type and tag what their Tauri steps type and tag; `tag-list-modal` adds the tags its Tauri steps add and opens the tag list with the choices its Tauri steps make (`camera` included by a click, `cat` excluded by a double click, the focus on the last tag clicked); only the `search-` scenarios and `tag-list-modal` carry tags; the three dock scenarios (`bulk-dock-tags`, `bulk-dock-group`, `group-order-modal`) make the clicks of `gallery-selection` and then, in the order of their Tauri steps, add `holiday`, or choose the group tab and type `trip`, and open the larger order view, and no other scenario uses the dock. The `kit` board is none of them. |
| `media_tagger_client::controls`, `icons`, `kit` | Each control's looks against the classes of its Solid source and the rules of `src/styles.css` (the tests read both and fail when a copied class changes): button variants with hover and disabled, icon buttons (active, heart, danger), chip tones, alert tones, the progress bar's track and clamped fraction, slider thumb and track, the segmented control, the DaisyUI `toggle-sm` measures, both spinners. The slider's keys (arrows, Page keys by a tenth of the range and at least a step, Home, End, nothing at an end), step snapping and pointer positions; the segmented arrows wrapping; the highlight track (port of the `UiSegmentedControl` case "slides selection but snaps changed layout and translation"); the thumbnail loader's delay and fade; the 22 icon names and asset paths and the filled variant; every text of the `kit` board resolves in all eleven languages. |
| `media_tagger_client::overlay` | The layer stack (Escape to the top layer only, a locked top layer, initial focus only for a modal still on top, the restore rules: covered by a modal, a newer modal, a nonmodal layer registered meanwhile, focus kept below a nonmodal layer, a missing target falling back to the next layer) and the modal presence (ports of the `UiModal` cases: a 100 ms exit that stays present, a stale finish on a rapid reopen, reduced motion, the snapshot of the open values, the four width presets against `UiModal.tsx`). |
| `media_tagger_gpui::cli` | No argument starts the application; `--identifier` stands alone; `--scenario` takes exactly one id; every other argument is refused with its text. |
| `media_tagger_gpui::scenario` | Without a scenario every profile starts in the default (dark) state; the E2E profile starts in every catalogued scenario, with its seed, tile size, selection clicks (`gallery-selection` and the three dock scenarios), tags (only the two `search-` scenarios and `tag-list-modal`), search header (only the two `search-` scenarios), open tag list (only `tag-list-modal`) and dock steps (only the three dock scenarios), and on the `kit` board; the dev profile refuses a scenario, `kit` included, before looking it up; an unknown id is refused with the known ids. `seed_library` on `FakeBackend`: the control seed scans the seeded directory as the one root, scenario tags go to the tiles of the unfiltered library by index, the empty seed touches nothing, a missing variable or directory is refused before the backend is called, and a refused scan fails the start. |
| `tests/gallery_grid.rs`, `#[gpui::test]` cases | The gallery grid on GPUI's test platform with `FakeBackend` (a synthetic library of any size, thumbnails held or answered with an 8 × 8 JPEG) and `ManualExecutor` (ports of the `GalleryGrid.test.tsx` cases that apply without selection, scrollbar and timeline): tiles fill the width in square rows below the header and only the range is drawn; a short result keeps the footer at the bottom of the viewport and does not scroll; the wheel moves rows by its distance and the second page is asked for once; End pauses page and thumbnail demand, draws still slots, and after 150 ms on the executor's clock the same range is demanded (its pages once, visible tiles at once, overscan as prefetch); tiles first shown during a pause ask for cached thumbnails only; a picture is drawn once decoded off the UI thread and videos and GIFs carry a chip; bytes that are no picture leave the tile empty; a click on a tile emits `GalleryEvent::OpenAsset`; Ctrl with the wheel changes the tile size instead of scrolling; a tile size change keeps the first visible asset at the top; the scroll keys; the load error banner above the empty state and its Retry, which Tab reaches and Enter and Space press, leaving the focus on the grid; leaving the gallery gives up the viewport and the thumbnail requests. `Thumbnails` alone: the picture cache stays within a 50-picture budget while 2,010 pictures pass through, and every dropped picture waits for its texture to be released. |
| `tests/gallery_position.rs`, `#[gpui::test]` cases | The position bar and the month timeline beside the grid (epic E10), same platform and fixtures: the bar spans the viewport 4 px from the right edge with a 32 px minimum thumb and is absent when nothing scrolls; a press beside the thumb centres it on the pointer without moving the focus, the drag follows the pointer outside the bar and clamps at the end; the focused bar scrolls by a row, a page and to the ends; the rail sits 24 px from the right edge, steps months and years with the keys, labels the current month while focused, and keeps reporting the month it jumped to when the end of the result clamps the scroll; a drag holds demand, jumps once per frame (`Window::simulate_next_frame`) and resumes on release; sections unavailable for the snapshot (ADR 0027) or a single month leave no rail. |
| `tests/gallery_position_render.rs`, `the_bar_and_the_timeline_draw_as_in_the_solid_gallery` | The bar and the rail through the headless renderer, read back as pixels in both themes, over 100,000 synthetic results in sixty months of five years: the idle track (`base-content/5`) with a 32 px `--border-strong` thumb at the top and at the end, the clear idle rail with its `primary/70` marker and 8 px `base-content/30` year ticks; under the pointer the rail's `base-content/5` fill, the year labels and the month label 20 px left of the rail's right edge, 26 px high (one `text-xs` line, `py-1`, a 1 px `--border-strong` border) on `--surface-solid`; after the keyboard the 2 px primary ring 2 px outside the bar and the rail. Skipped without a renderer unless `MEDIATAGGER_GPUI_REQUIRE_RENDERER` is set; with `MEDIATAGGER_GPUI_SCREENSHOT_DIR` set it leaves `gallery-position-<state>-<theme>.png` there. |
| `tests/gallery_selection.rs`, `#[gpui::test]` cases | Selection mode in the gallery (epic E11), same platform and fixtures, with ID ranges answered by the scripted backend: the bulk-actions toggle turns the mode on and off, holds a layer of the layer stack while it is on, and a click selects (check mark drawn) instead of emitting `OpenAsset`; Ctrl-click toggles and reads nothing, Shift-click reads the range from the anchor once, Ctrl+Shift adds it; a press that moves at once draws no rectangle, one held for 150 ms of the executor's clock does, with the tiles under it marked before the release, and the release selects them, replaces or (Ctrl) adds, and does not toggle the pressed tile with its own click; a rectangle starts in a gap and draws nothing outside selection mode; near the bottom edge frames (`Window::simulate_next_frame`) scroll the grid and the rectangle grows, clipped to the viewport; a long press of 500 ms emits `OpenAsset` and leaves the selection alone, never with a modifier; Ctrl+A selects 50,000 results as one range with only the keepalive read, does nothing outside selection mode, and Escape clears while the mode stays on; Retry on the banner repeats a range read that failed (the press on the button starts no gesture); selection mode and the selection survive the settings page, whose Escape closes only the page, while a range read on its way and a failed range with its Retry do not; the start state of `gallery-selection` selects its three tiles once. |
| `tests/gallery_grid.rs`, `frame_time_while_scrolling_a_large_library` (ignored) | A measurement, not a check: 600 frames of wheel scrolling through 100,000 synthetic results at the default and the smallest tile size and at 900 px a frame, each frame being the scroll step, the backend replies, the decodes and `Window::draw` on the test platform (element tree, layout and paint into a scene; no GPU). Run it in a release build; debug selectors are not recorded there, so the case uses none. |
| `media_tagger_gpui::profile` | Only `com.example.mediatagger.gpui.dev` and `.gpui.e2e` resolve; the three Tauri identifiers and publisher identifiers are refused with a message naming the identifier; the app-data directory follows `XDG_DATA_HOME`, then `HOME`. |
| `media_tagger_gpui::boot` | `start` on a temporary profile creates `instance.lock`, `media.db` and `thumbs/`; a second start on the same directory is refused until the first is dropped; two profiles run side by side; a corrupt database stops the startup, stays unchanged and does not keep the lock. |
| `media_tagger_gpui::window` | The display check: no `WAYLAND_DISPLAY` and no `DISPLAY` is an error instead of a headless start; a listening Wayland socket is found by absolute path and by name in `XDG_RUNTIME_DIR`; a socket file whose server is gone, a missing socket and a relative name without `XDG_RUNTIME_DIR` are refused; a dead `WAYLAND_DISPLAY` falls back to a live local X display and Wayland wins when both answer; an inherited `WAYLAND_SOCKET` and a non-local `DISPLAY` are not probed. The event loop: `a_window_that_cannot_open_ends_the_event_loop_with_its_error` runs GPUI's real Linux platform headless in a child process with an opener that fails, and requires the loop to end with `cannot open the main window: …` instead of running on without a window (the child is stopped after 30 s). |
| `tests/startup.rs` | The startup guards in the order `run` promises, through `run_with` with an injected identifier and environment: an identifier off the allowlist is refused before the display check and before anything is written; no display, and a display that does not answer, stop before the app-data directory exists; a relative data home is refused; a profile held by another instance stops before the window. Where the check has to pass, a socket in a temporary directory stands in for the compositor. The built binary: `--identifier` prints an allowed identifier, and without a display or with one that does not answer it exits with status 1, the `Tagrove GPUI cannot start` message and no app data. |
| `tests/scenario.rs` | Through `run_scenario_with`: the dev profile and an identifier off the allowlist refuse a scenario, and the E2E profile refuses an unknown one, before the display check and with no app data; a catalogued scenario on the E2E profile goes on to the display check. The built binary refuses `--scenario` unless it is the E2E build and refuses unknown or conflicting arguments, with status 1 and no app data. |
| `tests/scenario_window.rs` | The window a scenario starts in, for every entry of `SCENARIOS` on the E2E profile: the runtime starts in English and in the scenario's theme. On GPUI's test platform (`TestAppContext`) over an empty library, opening a scenario twice gives the same viewport (the 1440×900 capture size) and the same bounds of header, item count and empty state, and the light scenario lays out like the dark one. Needs no display; the pixels are checked by `shell_window`. |
| `tests/text_input.rs`, `#[gpui::test]` cases | The text field on GPUI's test platform, inside a stand-in owner with application shortcuts bound around it: a click focuses the field and typed text goes into it and to the owner as `Changed` events; movement, selection, deletion, undo and redo keys; copy, cut and paste through the clipboard, with pasted line breaks as spaces; Enter and Escape reach the owner and not the application, and an Escape the owner passes on falls through to the application's binding; shortcuts stay silent for keys that type a character and for keys the field binds, fire for a key it does not use (Ctrl+K), and all fire with the focus elsewhere; arrow up and down move the caret unless the owner captures them; a long text scrolls and the caret stays inside the field at every step; click, Shift+click, a drag that continues outside the field, double and triple click; composition through the input handler in UTF-16 (marked range, Enter during a composition, offsets past the end and inside a surrogate pair, bounds for the candidate window, also asked before the frame with the composed text is drawn, as the Linux backends do); the field has GPUI report the caret to the platform after a frame in which it took the focus or the text or the selection changed, and not otherwise (counted as next-frame callbacks; the test platform discards the bounds themselves); a disabled field gives up the focus and takes neither keys, pointer nor platform text; the owner's `set_text` and `replace_range`; the acceptance filter, and Ctrl+X ending a composition the filter refuses, which the owner hears of. |
| `tests/text_input.rs`, `the_field_draws_its_states_with_the_theme_tokens` | Three fields through the headless renderer, read back as pixels: field background and one-pixel border of an unfocused field with its placeholder, the primary border and the filled selection of the focused field, the dimmed disabled field, and a line wider than the field with the caret drawn at its right edge and nothing drawn outside the frame. |
| `tests/search_window.rs` | The search header of the gallery (epic E12) on GPUI's test platform with `FakeBackend` and `ManualExecutor`; the window is activated first, since focus events reach their listeners only in an active window. Ctrl+K focuses the field and typing opens the suggestion list without applying anything, Escape closes it and keeps the draft and the focus; arrows and Enter replace the active token keeping its `-`, a second Enter submits the parsed tags; a press on a suggestion picks it and keeps the focus; Search, favourites and a media kind each start one query with the composed filters and Clear resets them and focuses the field; a refused submit (`gN:`) shows its message until the draft changes; a group name opens no list and Enter submits it whole. The `search-suggestions` start state in a window that stays inactive (as on the comparison display) still opens the list, and the frame keeps its normal border until the window is active; the input and the list start 37 pixels into the field and the Ctrl K hint is 16 pixels high, as measured in the Tauri capture. `the_suggestion_list_renders_below_the_field` draws the `search-suggestions` start state through the headless renderer (first option in the active colour), and `the_tools_divider_is_drawn` checks that the divider before the tag list's button fills its pixel column. |
| `tests/tag_list_window.rs` | The tag list (epic E13) in the main window with `FakeBackend` and `ManualExecutor`, delivering the next frame after each step (a dialog takes and gives back the focus one frame later). The launcher in the tools row opens the dialog with the focus in its filter and asks for the first page of 100; a click includes a tag only after the 220 ms timer, a double click (a second press with click count 2) excludes one, Apply starts one query with the chosen tags, closes the dialog, writes `camera -cat` into the search field and gives the focus back to the launcher; Escape closes the dialog and opening it again shows an empty filter; Shift+Enter and Shift+Space on a focused tag exclude and include it and keep its focus. The `tag-list-modal` start state opens the dialog with `camera` included and `cat` excluded. A frame draws only the rows near the visible part of the list (`drawn_chips`): with 1,000 tags loaded the top of the list draws the same buttons as with 100, the end draws the last tag and not the first, and a drawn tag still takes hover and clicks; the drawn rows are the rows of `flex flex-wrap gap-1.5` (each button inside the list, one gap between buttons and between rows, a new row only for a button that does not fit, a tag wider than the list on a taller row of its own). |
| `tests/tag_list_window.rs`, `frame_time_while_scrolling_and_hovering_a_long_tag_list` (ignored) | A measurement, not a check (review findings r2-perf-1, r4-gpui-3, r6-gpui-1 of E13): with about 1,000, 5,000 and 20,000 tags loaded by scrolling to the end, 300 frames each of an unchanged redraw, of wheel scrolling 40 px a frame and of the pointer moving over the tags, each frame being the event, the work it queues and `Window::draw` on the test platform (no GPU; text shaped by GPUI's `NoopTextSystem`); it prints mean, p95 and max. Run it in a release build, where no debug selector is recorded. |
| `tests/tag_editor_window.rs` | The shared tag editor (epic E13, `TagEditor.test.tsx`) in a window of its own with a runtime that knows the tags `cat` and `car`: chips and the field in one frame and a press on its blank padding focuses the field; a remove button asks the owner without focusing the field; disabled and loading editors refuse focus, typing, Enter and removal; assigned tags are never suggested, Enter adds the draft, Down and Enter hand the picked tag to the owner and the list stays open; disabling closes the list and enabling recovers it; the exclusions follow new props; Escape closes the list and keeps the draft and the focus. |
| `tests/bulk_dock_window.rs` | The bulk dock (epic E14) in the main window with `FakeBackend` (seven images) and `ManualExecutor`, delivering a few frames after each step. Selection mode shows the 100-pixel chip at the bottom centre of the gallery's viewport, a selection expands it to the dock's width, the counter clears the selection and keeps the mode, and turning the mode off drops the dock; a press on the dock's own padding changes nothing on the grid beneath; one selected asset shows its complete tag list with remove buttons, a removal writes the list without that tag and shows the backend's canonical list, and the heart toggles the favorite of the selection; a tag typed for three assets is normalized and merged once, shown as a chip without a remove button, the field is emptied and focused again, and an invalid draft stays with its message and sends nothing; the tabs follow clicks, Left (wrapping), End and Home with the focus, the group tab has the tall height on the same bottom edge, the keys scroll nothing beneath, and clearing the selection returns to the Tags tab; a group key shows the inline board, Right on a grip moves its tile and keeps the grip's focus, and Apply sends the inline order with orders 1 to n; a tile dragged onto another shows the insertion line and takes its place on release; the larger view starts from the inline draft, replaces the inline board, drops its changes on Cancel and saves exactly its order, after which the inline draft restarts from the saved one. The three dock scenarios start with their tag added and no field focused, or with the key typed and the board inline or in the larger view, in the gallery's order of the three tiles (the Shift click replaces the selection with their range), and save nothing. The group key row keeps the Solid grid's 40-pixel track for the "Generate UUID" button (8 pixels after the field, Apply 48 pixels after the button's left edge and at the panel's right edge). Escape in the tag field closes its suggestions while they show items and otherwise clears the selection, Escape in the group key field clears the selection, and selection mode stays on in both. A publish of the runtime that changes nothing leaves the group key field's text, selection and focus alone and makes it report nothing. |
| `tests/bulk_dock_ported.rs` | The view cases of `BulkDock.test.tsx` and `GroupOrderModal.test.tsx` in the main window with `FakeBackend` and `ManualExecutor`; a pending save is a call the test has not run. The tag field gets the focus back after its save ended even when the focus moved meanwhile, a retired selection's save neither clears the draft nor takes the focus, the tag failure and the partial count show on both tabs, the header row holds the field, the heart and the counter in that order, a failed tag read keeps editing disabled until Retry, the tags show their loading state, a group conflict shows on the group tab only, Cancel and Escape give the focus back to "Sort in larger view", a saving larger view ignores Escape, Cancel and a second Save, the closing view keeps its last order until it is gone, a thousand selected items draw and request pictures only near the view, and a tile moved by keys stays drawn and in view. One selected asset loses a tag through its chip's remove control (named "Remove tag …"), gains the typed draft on Enter and a suggestion by a press; the suggestion list lies above the field, reaches outside the dock's box and takes the press there, so it is not clipped to the dock. |
| `tests/search_ported.rs` | The search header's view cases of `SearchTagatorWrapper.test.tsx`, `SearchTagator.test.tsx` and `suggestionRecovery.test.tsx` on GPUI's test platform: Ctrl+K and Meta+K focus the field, not with Shift or Alt and not under a modal layer; the media kind and favourites each submit once, a refused Search starts nothing and Clear gives the focus back; the list hangs below the field and closes on a press outside, on Tab and on a blur (the drawn frame no longer holds it); Retry after a failed suggestion search (a dropped executor task) works by pointer and by Tab and Enter and gives the focus back; the draft, kind and favourites survive a remount of the gallery; with 100,000 known tags a keystroke only queues the matching on the executor. |
| `tests/text_input_owner.rs` | The text field inside a stand-in for the tag editor (a chip's remove button before the field, a button after it, a suggestion list that takes the arrow keys while it is open), with application shortcuts bound on the keys the field uses and Tab and Shift+Tab bound to the focus order. The cases of `src/features/tagging/__tests__/TagEditor.test.tsx` that are about the field: Enter submits the typed draft to the owner and the arrow keys walk the owner's list without moving the caret; Tab moves from the chip to the field and on, Shift+Tab back, and Escape closes the list while the draft and the focus stay; a disabled field takes no typing, keys or click, is skipped by Tab, and submits its kept draft once it is enabled again. From the acceptance criteria: every key the field binds stays silent for the application while the field has the focus, even where the key changes nothing (an empty field, Delete at the end), and each fires with the focus on the chip; a long value set by the owner scrolls with the caret visible and a click in the scrolled line puts the caret under the pointer; the candidate-window bounds of a composition in a scrolled line lie inside the field, at the start of the composition when asked before the next frame and ending at the caret after it; a composition ends when the focus leaves, its text stays, reaches the owner and is one undo step; copy and cut take a ZWJ sequence whole and an empty clipboard pastes nothing; a click anywhere on an emoji puts the caret at one of its edges. |
| `tests/overlays.rs` | The layer stack and `Modal` on GPUI's test platform, with two dialogs and a trigger inside `layers::root` (ports of `UiModal.test.tsx` and `UiLayerProvider.test.tsx`): Escape closes the top dialog, then the next, then nothing; a top dialog that closes on neither Escape nor the backdrop keeps both from every layer; a press inside the dialog does not dismiss it and one on the backdrop dismisses only the top one; a dialog takes the focus on its first tab stop one frame after it opens, Tab and Shift+Tab wrap inside it and never reach the trigger, and Escape gives the focus back to the trigger; closing the upper dialog restores its named target in the lower one while it still fades out; a closing dialog ignores the backdrop and stays drawn for exactly the 100 ms exit; a dialog opened while only the root had the focus gives the focus to the root when it closes, so Enter and Space during its exit activate none of its buttons. Next frames are delivered with `Window::simulate_next_frame`, time with the executor's clock. |
| `tests/controls.rs` | The slider and the segmented control inside an owner (ports of `controls.test.tsx`): the slider answers arrows, Page keys, Home and End and sends nothing at an end, and keeps its value when the owner refuses one; each segmented group is one tab stop, arrows wrap and ask for the segment they land on, groups are independent, and a group whose owner refuses keeps showing the owner's value while the focus moves, asking only for segments that are not selected. The segmented highlight is drawn on the frame after the group first shows and moves to the segment an arrow key selected without any other event (`segmented_highlight`). A long message wraps at the inner width of a narrow alert in a plain box, in a column with a gap, and with a title and a button, which keeps its own width under the message (`alert_layout`). A button's `size` is its width and its height unless a `height` is given (`button_sizes`). |
| `tests/kit.rs` | The `kit` board opens in the 1440×900 window; through the headless renderer its left half has the dark and its right half the light background (`the_board_renders_through_the_headless_renderer`), and with its small dialog open the dialog's surface is drawn at the window's centre and the backdrop dims the background in the far corner (`a_dialog_of_the_board_draws_over_the_whole_window`); an active icon button and a disabled primary button show their fill over the background without their outer shadow showing through (`a_translucent_button_shows_the_background_and_not_its_shadow`). |
| `tests/shell_window.rs`, `#[gpui::test]` cases | The shell on GPUI's test platform with `FakeBackend`, `ManualExecutor` and `MemoryPrefsStore` (ports of `shell.test.tsx`, `bootstrap.test.tsx`, `startupScanRemount.test.ts` and `WindowControls.test.tsx` where they apply): the main window options carry the profile's title, application id, minimum size and client-side decorations; the start asks for the library, the startup scan and the roots once and draws header, count, empty state, the three window buttons and the eight grips; switching to the settings page and back keeps the runtime entity and its tile size and makes a new gallery view whose slider shows that size, with no backend call; Escape closes the settings page and gives the focus back to the settings button; a failed startup scan leaves the window usable and is not asked again; a scan with work refreshes the count; theme and language switch live and are saved; the close button closes the window, by a click and by Tab from the settings button to it and Enter. |
| `tests/shell_window.rs`, `the_empty_shell_renders_through_the_headless_renderer` | The empty shell in both themes through `HeadlessAppContext` with GPUI's wgpu renderer: two captures of the same state are identical, the header has the chrome colour and the gallery the application background. |
| `tests/shell_window.rs`, `the_window_text_uses_the_bundled_fonts` | The fonts of the window through the real platform text system (`HeadlessAppContext`, no renderer needed): after `window::init_application` the shell's root text style names the sans family of the typography tokens, and Inter and JetBrains Mono each resolve to a face of their own, not to GPUI's fallback stack, with that face's advance widths. Fails when the startup path stops registering the bundled fonts. |
| `tests/shell_ported.rs` | The shell cases of `shell.test.tsx`, `WindowControls.test.tsx` and `bootstrap.test.tsx` driven through clicks, keys and focus on GPUI's test platform with `FakeBackend`, `ManualExecutor` and `MemoryPrefsStore`: "Add first folder" highlights scanning until Back; theme and language change live, are saved, and survive Escape and reopening the settings page; Escape from the page gives the focus back to the settings button and leaves no layer; the window is drawn in its theme and switches views before the startup scan answers, which is asked once; the tile size set with the slider's keys survives a view switch; fullscreen removes the eight resize grips until it ends; secondary presses and presses on header controls never move the window; a primary drag on the header starts a window move, a double click toggles maximise, and the minimise and maximise buttons ask the window (each seen as the test window's `unimplemented!`, so those cases are `should_panic`); a secondary click opens no menu; the window opens on the gallery with identity, settings button and count in its header and the window buttons last. |

The eight headless-renderer tests (`the_empty_shell_renders_through_the_headless_renderer`, `the_suggestion_list_renders_below_the_field`, `the_tools_divider_is_drawn`, `the_field_draws_its_states_with_the_theme_tokens`, `the_board_renders_through_the_headless_renderer`, `a_dialog_of_the_board_draws_over_the_whole_window`, `a_translucent_button_shows_the_background_and_not_its_shadow` and `the_bar_and_the_timeline_draw_as_in_the_solid_gallery`) need a wgpu adapter. A GPU works without a display, and so does Mesa's software rasterizer through EGL, which is what a sandbox without `/dev/dri` uses. With no adapter at all each test prints `skipped` and passes; set `MEDIATAGGER_GPUI_REQUIRE_RENDERER=1` to make that a failure. `MEDIATAGGER_GPUI_SCREENSHOT_DIR=<existing directory>` makes the `shell_window` test write `shell-empty-headless.png` and `shell-empty-light-headless.png` there, the `text_input` test `text-input-states.png` and `text-input-scrolled.png`, and the `kit` tests `kit-headless.png`, the board of every shared control in both themes for review against `src/styles.css`, and `kit-dialog-headless.png`, the same board with its small dialog open, and the `gallery_position_render` test `gallery-position-{idle,rail-hover,bar-focus-end,rail-focus}-{dark,light}.png`, the right edge of the gallery for review against `GalleryScrollbar.tsx` and `GalleryTimelineScrubber.tsx`, and the `search_window` test `search-suggestions-headless.png`.

### Screenshots of the GPUI window without a real display

Two paths work on a machine whose only Vulkan driver is a hardware one, with no extra package:

1. **In-process** (preferred for tests): `HeadlessAppContext::with_platform(text_system, assets, || gpui_platform::current_headless_renderer())`, open the window, `run_until_parked`, `capture_screenshot`. It needs the `test-support` feature of `gpui` and `gpui_platform`, which only the dev-dependencies enable, so it is available to test targets and not to the shipped binary. The capture is at the test platform's scale factor 2 (2880×1800 for the 1440×900 window).

   ```bash
   MEDIATAGGER_GPUI_SCREENSHOT_DIR="$(mktemp -d)" MEDIATAGGER_GPUI_REQUIRE_RENDERER=1 \
     cargo test --manifest-path src-gpui/Cargo.toml --locked -p media_tagger_gpui --test shell_window \
     the_empty_shell_renders_through_the_headless_renderer -- --nocapture
   ```

2. **The real binary on a private Xvfb display**, captured with `import -window root` as `app:control` does for Tauri. Xvfb has no DRI3, so the hardware Vulkan driver cannot present there; GPUI falls back to EGL with Mesa's software rasterizer and the window is drawn correctly. Use scratch XDG directories (mode 700) so that the profile, the D-Bus session and the portal state stay out of the real session, and run one display-bound command at a time:

   ```bash
   unset WAYLAND_DISPLAY
   dbus-run-session -- xvfb-run -a -s '-screen 0 1440x1100x24 -nolisten tcp' sh -c '
     src-gpui/target-e2e/debug/media_tagger_gpui & pid=$!
     # wait until a window titled "Tagrove GPUI E2E" is viewable, then:
     import -display "$DISPLAY" -window root "$XDG_RUNTIME_DIR/gpui.png"
     # ask the window to close (WM_DELETE_WINDOW) or stop the process you started:
     kill -TERM "$pid"; wait "$pid"'
   ```

   The window's title is the profile title and its `WM_CLASS` is the identifier. Sending `WM_DELETE_WINDOW` closes the window and the process exits with status 0. Stop only the process ID you started, never by name.

A real screen is never used for captures. The [comparison harness](#comparison-harness-tauri-and-gpui) uses the second path.

### Comparison harness (Tauri and GPUI)

`bun run compare:ui <scenario> [<scenario> ...]` captures the same named state of both applications on the same synthetic data, so that the screenshots can be compared by eye or by an agent (plan decision D10: layout, tokens and content, not pixels). `bun run compare:ui --list` prints the scenarios it knows. For each scenario it leaves, in the ignored `artifacts/gpui-compare/<scenario>/`:

| File | Content |
| --- | --- |
| `tauri.png` | The private 1440×900 display with the Tauri window, taken through `app:control`. This is the reference. |
| `gpui.png` | The private 1440×900 display with the GPUI window, taken through `gpui:control`. |
| `side-by-side.png` | Both captures joined left to right (Tauri, then GPUI). |
| `result.json` | Scenario id, description (from `docs/plans/gpui-port/epics.json`), theme, seed, and per side `ok`, file, size, window title and the pixel signature (`magick identify -format %#`), or the error. It holds no time and no absolute path, so two runs that draw the same pixels write the same file. In an error the checkout, the temporary directory and the home directory read `<checkout>`, `<tmp>` and `<home>`, and any other absolute path reads `<path>`; the console output of the run keeps the full text. An error still names its control session, so it can differ between two failed runs. |

The scenario directory is emptied at the start of a run, so a rerun leaves the same set of files. When a Tauri step fails, the display at that moment is kept as `tauri-failed.png`, `result.json` records the error, no `side-by-side.png` is written, and the command exits with status 1 after the remaining scenarios.

A **scenario** is one file `e2e/compare/scenarios/<id>.js` with `{ id, theme, seed, tauri }`, an entry in `SCENARIOS` of `src-gpui/crates/media_tagger_client/src/scenario.rs`, and an entry in `scenarios` of `docs/plans/gpui-port/epics.json`; `e2e/compare/scenarios.test.js` and the unit tests of `scenario.rs` keep the three in step, including the theme.

- **Tauri side.** `app:control start` always begins in English with the dark theme and its own seed indexed (four PNGs, one GIF, two MP4s). `tauriSteps` in `e2e/compare/scenarios.js` turns the declared state into `app:control` commands: the light theme is chosen on the settings page, `seed: "empty"` removes the seeded scan root through the settings page, and a reload follows, so the capture shows what a fresh start on that profile shows. The scenario's own `tauri` steps come next (`[command, ...arguments]`, validated by `validateRequest` of `e2e/control-support.js`; only acting commands, `wait` and `refresh`), then `wait idle`. `e2e/control.js` itself is unchanged.
- **GPUI side.** `media_tagger_gpui --scenario <id>` starts the application in the state of that id. Only the E2E build accepts it: `scenario::start_state` refuses a scenario on the dev profile and an unknown id before the display check, so nothing is written. There is no input driver for the GPUI window; a scenario is a start state.
- **Seed.** `e2e/compare/seed.js` builds the media directory of a GPUI scenario from `e2e/fixtures.js` with the file names `app:control` uses (`control`), or leaves it empty (`empty`), in the session's scratch root. Its path is exported to the application as `MEDIATAGGER_GPUI_SCENARIO_MEDIA`. `shell-empty` and `shell-empty-light` use the empty seed. A scenario with the `control` seed (`gallery-default`, `gallery-tiles-small`, `gallery-tiles-large`, `gallery-selection`) indexes that directory before its window opens: `scenario::seed_library` scans it as the library's one root through the `Backend` port (`scan_folder`), as `app:control start` indexes the same files for Tauri, and a missing or invalid directory fails the start. Thumbnails are rendered on demand once the tiles ask, so the capture waits for the display to settle. The Tauri steps of the two tile-size scenarios focus the native range input with a click, press Home or End, and click the item count to take the focus off the slider again; the GPUI side starts with that tile size (`Scenario::tile_size`). `gallery-selection` turns bulk mode on and clicks the second tile, Control-clicks the first and Shift-clicks the third (`click` and `modclick` on `button[data-asset-index]`), which leaves the first three tiles selected, then clicks the item count to take the hover off the tiles; the GPUI side makes the same three clicks on its selection model as soon as the rows of those tiles are loaded (`Scenario::selection`, `scenario::selection_inputs`, `Runtime::script_selection`). Both captures show the expanded bulk dock on its Tags tab. `search-suggestions` gives the first tile the tags `camera`, `car` and `cat` (Tauri: through the bulk dock with bulk mode turned off again; GPUI: `seed_library` reads the unfiltered library once and calls `set_asset_tags` before the window opens, `Scenario::tags`) and types `ca` into the focused search field, so the suggestion list is open with its first option active; `search-applied` types `-cat` and chooses the Images segment, which submits both, so the count shows the four images (GPUI: `Scenario::search`, `Runtime::script_search`). `tag-list-modal` gives the first tile the same three tags, opens the tag list from the tools row, clicks `camera` (included once its 220 ms timer fired) and double-clicks `cat` (excluded); the GPUI side opens the runtime's tag list and makes the same choices with double clicks, which need no timer (`Scenario::tag_list`, `ScenarioTagList::inputs`, `Runtime::script_tag_list`), and the dialog follows the open list as soon as the gallery view exists. The three dock scenarios make the clicks of `gallery-selection` and click the item count before they touch the dock, so no tile keeps the pointer's hover: `bulk-dock-tags` types `holiday` into the dock's tag field and presses Enter, which leaves the tag as an add-only chip; `bulk-dock-group` clicks the Group & order tab and types `trip` into the key field, which shows the inline order board of the three tiles; `group-order-modal` also clicks "Sort in larger view". Nothing is applied in the two group scenarios. Which seeded file lies at which gallery position is not pinned on either side: the gallery sorts by modification time in whole seconds and then by asset ID, the seed is written within about a second, and a scan gives IDs in the order its workers finish. Two runs of one application can therefore show different file names in the same places (the third card of the order board, the order of the two videos), so compare the board with the gallery of the same capture, not with the other application's file names. The GPUI side makes the same steps on its dock once the clicks are made and their range is read (`Scenario::dock`, `Runtime::script_dock`, `Runtime::take_scripted_dock`): the tag through the dock's own add, the tab and the key at once, the larger view as soon as the inline board is there. No field gets the focus by a scripted step, since a capture of the Tauri window on its private display shows neither a caret nor a focus ring.

The two applications never run at the same time. Every Tauri capture uses its own `app:control` session (start, steps, screenshot, stop), because the steps change the profile; then one GPUI session serves all scenarios. Each `app:control start` rebuilds the frontend and relinks the E2E binary, so a scenario costs about a minute on the Tauri side.

#### `bun run gpui:control`

`e2e/compare/gpui-control.js` is the counterpart of `app:control` for the GPUI adapter, without input commands:

```sh
bun run gpui:control start [--scenario <id>]   # keep it running; prints BUILD, then READY with the session ID
bun run gpui:control scenario <session> <id>   # restart the application in a named state
bun run gpui:control screenshot <session>      # capture the private display
bun run gpui:control doctor <session>
bun run gpui:control logs <session>
bun run gpui:control stop <session>
```

- **Guards.** `start` refuses before it builds or locks anything when `scripts/gpui-build-e2e.mjs` does not name the identifier `com.example.mediatagger.gpui.e2e`, the title `Tagrove GPUI E2E`, the target directory `src-gpui/target-e2e` and the binary inside it (`validateGpuiIdentity`). After `bun run gpui:build:e2e` it asks the binary for its baked identifier again. The doctor, which runs before `screenshot` and after `scenario`, requires the live window to carry that title, the identifier as its window class and the process ID of the application the session started, and the profile to be open inside the session's scratch root.
- **One display-bound run at a time.** `start` and `compare:ui` refuse while `artifacts/app-control/active.lock` exists or while the wdio launcher of `bun run test:e2e:tauri` runs in this checkout (found in the process table: the `wdio` CLI with this checkout's `e2e/wdio.conf.js`, resolved against its working directory). `start` also refuses while a desktop driver port (4444, 4446, 4447) is taken, and checks all of this again once it holds its lock. The session's own lock is `artifacts/gpui-control/active.lock`; `compare:ui` refuses while it exists. A lock is only ever removed by the session that wrote it, and only while it still names that session; no process is signalled by name.
- **Isolation.** Each session gets a new root `media-tagger-gpui-control-*` under `realpath(os.tmpdir())` with `data`, `config`, `cache`, `runtime` (mode 700) and `media`, and an authenticated Xvfb (`1440x900x24`, TCP disabled) on an explicit display from `:90` up, never `:0` or `:1`. The application runs with those XDG directories, without `WAYLAND_DISPLAY`, and with a D-Bus address inside the scratch root where no bus listens, so it does not reach the desktop portal of the user's session. No `dbus-run-session` is needed. `scenario` stops the application, removes only `<scratch>/data/com.example.mediatagger.gpui.e2e` and `<scratch>/media`, seeds, and starts it again.
- **Readiness.** A launch waits until a window with the E2E title has its class and process ID set, then until two captures 200 ms apart have the same pixels and differ from the display before the launch. The window exists before its first frame is presented, so the title alone is not enough. Xvfb has no DRI3; GPUI draws through EGL with Mesa's software rasterizer there.
- **Captures** go through `assertPrivateDisplay` of `e2e/display.js` with a display and authority file taken from the session manifest only.
- **Stop.** `stop`, SIGINT, SIGTERM and SIGHUP stop the owned process groups, remove the marked scratch root and then the lock; once a stop has begun the session spawns nothing more, so an interrupted start does not retry its Xvfb or launch the application; evidence (`session.json`, `process.log`, `actions.jsonl`, screenshots) stays under `artifacts/gpui-control/<session>/`. A signalled `start` exits with 129, 130 or 143. `compare:ui` starts the session over an IPC channel, and the session stops itself the same way when that launcher disappears. `compare:ui` stops the session it started on every path, including a failed step and its own SIGINT/SIGTERM/SIGHUP (exit 129, 130 or 143). If owned processes cannot be stopped, the scratch root and the lock stay for inspection, as with `app:control`.

Prerequisites beyond those of `app:control`: `xprop` and ImageMagick's `magick`.

`bun run test:compare` (`node --test e2e/compare`, which loads every `*.test.js` of the directory through `e2e/compare/index.js`) needs no display. It covers the identity guards, the scratch and profile cleanup guards, the lock, the application and X-tool environments, the window identity parser, the launcher lookup against fake process tables, the scenario catalogue and its agreement with `epics.json` and the Rust catalogue, the generated Tauri steps, the seeds (ffmpeg), the result file (including that the error of a failed side reaches it without an absolute path), and `e2e/compare/guards.test.js`, which runs both commands in a disposable copy of the checkout with a fake `bun` and requires a refusal with nothing built, locked or started when an `app:control` lock, a GPUI lock or a running desktop E2E launcher is present, or when the build script names another identifier, title, target directory or binary. One case of that file lets `start` run up to its Xvfb with stand-ins for the build, the binary and the X tools (an `Xvfb` that never reports ready), interrupts it with SIGTERM and requires exit status 143, no second Xvfb, no process, no scratch root and no lock. `start` probes the machine's driver ports (4444, 4446, 4447) before its own lock, so that case and the GPUI-lock case report `skipped` while another program holds one of those ports.

Not covered by an automated test: the session with the real tools (Xvfb, the built binary, capture, stop and signal cleanup after readiness), which needs a display and a build. It was verified by running `bun run compare:ui shell-empty shell-empty-light` twice (same files and the same four pixel signatures), by interrupting `gpui:control start` with SIGTERM, `compare:ui` with SIGTERM during its Tauri session and with SIGKILL during its GPUI session, and checking each time that no lock, scratch root, Xvfb or application process was left. A SIGKILL of `compare:ui` during its Tauri session leaves the `app:control` session running; stop it with `bun run app:control stop <session>`.

## Packaging checks

`bun run test:packaging` verifies lockfile source generation, publication identity rejection, checksum inventory, duplicate-bundle rejection, and failed-build preservation with disposable artifacts. Docker builds perform format-specific artifact checks. See [package verification](linux-packaging.md) for installed-package tests and remaining desktop coverage.

## Real desktop E2E

Release production is documented in [setup and builds](setup-and-build.md#docker-packages). Its build does not run test suites.

Specs that need the library contents for assertions call `listAssetsForAssertions` from `e2e/assets.js` (it replaced the removed `list_assets` command, [ADR 0009](../adr/0009-remove-list-assets-command.md)). It runs a query session under its own owner (`e2e-assertions`) and returns `{ items, total }` with `AssetDetails` items in gallery order. A start from any owner supersedes a start that is still running, so the helper waits until the app has no `start_asset_query` in flight and presses the gallery's Retry if it still interrupted one; call it while the gallery is idle when possible.

Specs without an opt-in flag run on every `bun run test:e2e:tauri`: `app.smoke`, `app.workflows`, `group-order`, `review-fixes`, and `startup-scan`. The others skip unless their flag is `1`:

| Spec | Flag |
| --- | --- |
| `about` | `MEDIATAGGER_ABOUT` |
| `animations` | `MEDIATAGGER_ANIMATIONS` (plus `MEDIATAGGER_NATIVE_DISPLAY`) |
| `gallery-browsing` | `MEDIATAGGER_GALLERY_BROWSING` |
| `gallery.performance` | `MEDIATAGGER_GALLERY_PERF` (plus `MEDIATAGGER_NATIVE_DISPLAY`) |
| `gallery-100k` | `MEDIATAGGER_GALLERY_100K` |
| `gallery-selection` | `MEDIATAGGER_GALLERY_SELECTION` |
| `gallery-timeline` | `MEDIATAGGER_GALLERY_TIMELINE` |
| `grid-baseline` | `MEDIATAGGER_GRID_BASELINE` |
| `lightbox-activity` | `MEDIATAGGER_LIGHTBOX_ACTIVITY` (plus `MEDIATAGGER_NATIVE_DISPLAY`) |
| `lightbox-images` | `MEDIATAGGER_LIGHTBOX_IMAGES` |
| `lightbox-recovery` | `MEDIATAGGER_LIGHTBOX_RECOVERY` |
| `native-video-escape`, `video-speed` | `MEDIATAGGER_NATIVE_VIDEO` (plus `MEDIATAGGER_NATIVE_DISPLAY`) |
| `search-recovery` | `MEDIATAGGER_SEARCH_RECOVERY` (plus `MEDIATAGGER_NATIVE_DISPLAY`) |
| `selection-recovery` | `MEDIATAGGER_SELECTION_RECOVERY` |
| `settings-language` | `MEDIATAGGER_SETTINGS_LANGUAGE` (plus `MEDIATAGGER_NATIVE_DISPLAY`) |
| `settings-maintenance` | `MEDIATAGGER_SETTINGS_MAINTENANCE` |
| `startup-scan-remount` | `MEDIATAGGER_STARTUP_REMOUNT` |
| `tag-editor` | `MEDIATAGGER_TAG_EDITOR` |
| `thumbnail-fade` | `MEDIATAGGER_THUMBNAIL_FADE` |
| `ui.redesign` | `MEDIATAGGER_UI_REDESIGN` (plus `MEDIATAGGER_NATIVE_DISPLAY`) |

### Linux prerequisites

1. Install the repository dependencies, normal Tauri build prerequisites, libmpv/GTK/OpenGL stack, ffmpeg, and ffprobe.
2. Install `tauri-driver`:

   ```bash
   cargo install tauri-driver --locked
   ```

3. Install WebKitWebDriver, or set `WEBKIT_WEBDRIVER_PATH` to its full path.
4. If `tauri-driver` is not at `~/.cargo/bin/tauri-driver`, set `TAURI_DRIVER_PATH` to its full path.
5. Put ffmpeg on `PATH` or set `FFMPEG_PATH` so the mixed-media workflow can generate temporary GIF and MP4 fixtures. The two playable MP4s have no audio tracks; the playback-responsiveness case generates one 60-second MP4 with a silent AAC track. For `test:app-control`, put ffprobe on `PATH` or set `FFPROBE_PATH`.
6. Run under a graphical session with working GTK/OpenGL, and ensure TCP port `127.0.0.1:4444` is free.

The harness runs `media_tagger` with WebKitWebDriver. When running the suite through Xvfb from a Wayland session, unset `WAYLAND_DISPLAY` and set `GDK_BACKEND=x11`; setting `DISPLAY` alone does not force GTK onto the private display. Only the E2E window profile permits resizing down to 320 × 240 so responsive lightbox behavior can be tested; dev and release retain their 1000 × 720 minimum.

Native screenshots read the whole X screen with ImageMagick `import -window root`, which on a developer desktop would copy unrelated windows into `artifacts/`. Specs therefore capture only through `captureDisplay` in `e2e/display.js` (`animations`, `gallery.performance`, `lightbox-activity`, `native-video-escape`, `settings-language`, `ui.redesign`, `video-speed`), and their `before` hooks call `assertPrivateDisplay` first. The guard throws, and never skips, unless `DISPLAY` is a local `:N` equal to `MEDIATAGGER_NATIVE_DISPLAY`, is not `:0` or `:1`, `XAUTHORITY` is absolute, and a live `Xvfb` owned by the current user serves that display with exactly that authority file, passed as `-auth` (`app:control`, older `xvfb-run`) or as `XAUTHORITY` in the Xvfb environment (newer `xvfb-run`). Run these specs with the headless recipe that exports `MEDIATAGGER_NATIVE_DISPLAY="$DISPLAY"` inside `xvfb-run`. The standalone `e2e/gallery-scale/webkit.py` tool is not a spec and does not use this guard.

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
8. Before tests run, the suite reads the live window title and refuses to continue unless it exactly matches the E2E title. WebdriverIO logs and ignores an error thrown from the `before` hook, so on a mismatch (or an unreadable title) the guard stops `tauri-driver` and ends the worker with exit status 78 before any spec or preference write, and the launcher's `onWorkerEnd` hook then ends the whole run with status 1 so no later spec file starts another worker.
9. `afterSession` and process signal handlers stop `tauri-driver`. An interrupted run exits with the conventional signal status (130 for SIGINT, 143 for SIGTERM, 129 for SIGHUP) instead of 0, so `&&` chains such as `test:all` stop (the handlers are prepended because the `create-wdio` SIGINT handler that the WDIO CLI loads exits with 1 synchronously and would otherwise run first). A worker whose launcher disappears stops its own `tauri-driver` and exits with status 1 instead of staying orphaned.

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

Only then does it recursively remove the E2E directory with retries. An `EPERM` lock is a limitation: the runner warns, skips that cleanup, and continues, so stale isolated E2E state may remain. The live-title check is the final guard before specs containing destructive actions; it ends the run with a non-zero status rather than throwing.

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
tag/group actions. After the bulk tag is saved it searches for the tag and waits until the
rendered tiles are exactly the two selected assets (the unfiltered gallery must show more). Its input PNGs are public repository icons; source files are not deleted.
It does not prove native playback or native file pickers. Keep logs/screenshots in ignored
`artifacts/`. After the owned app/driver/display processes exit, check for session-owned FUSE
mounts under the printed temporary root, unmount any that remain, and remove only that root.
A failed run retains the root for inspection; never substitute a development/release profile.

### Fixtures, selectors, and destructive workflows

`app.smoke.e2e.js` indexes the checked-in PNG assets under `src-tauri/icons` and exercises navigation, settings, bulk tags, and bulk groups. It does not delete those source files. `app.workflows.e2e.js` creates unique roots with `fs.mkdtemp`, uses `e2e/fixtures.js` to copy valid PNG icons or use ffmpeg to generate a small GIF and two MP4 files without audio tracks, and records temporary CSV/ZIP artifact paths under the operating-system temp directory. Its `afterEach` clears the isolated library database and removes only those roots and artifacts. Video coverage includes picture clicks to pause/resume and focused Left/Right seeking by five seconds, plus native IPC checks for paused seeking, fullscreen session continuity, retained preferences, autoplay on replacement, and stale closes. These checks use backend snapshot attributes, not GTK button clicks or video pixels. Other video coverage checks authorized native opening, the full native-player footprint, absence of a DOM control rail, rapid source switching, absence of an HTML `<video>`, error fallback, and native-surface cleanup. One case opens a 60-second MP4 with a silent audio track (`createAudioVideoFixture`), long enough that playback cannot loop during the check, and requires ten consecutive `list_scan_roots` round trips to finish within two seconds each while playback advances. It fails when native redraws starve async IPC responses. The private runtime directory of the documented run has no audio server, so there the clip plays through libmpv's null output; with the host's `XDG_RUNTIME_DIR` it opens a real output and stays inaudible. WebDriver cannot inspect pixels or controls rendered by GTK above the WebView.

The workflow suite deliberately tests library clear, scan-root removal, backup restore, and permanent media deletion. Keep every deletable media fixture under a newly created temp root. Never change a destructive spec to index a personal directory, the repository root, or production app data. Do not weaken the identifier, target-directory, app-data-path, or live-title guards to make a failing run proceed.

Desktop selectors currently mix stable IDs/classes/data attributes, `aria-label` values, visible English text selectors such as `button=Apply`, and a small number of XPath expressions. Prefer stable accessible roles/names or explicit test IDs when text is not itself the behavior. If visible copy or accessibility labels change, update the associated E2E selectors in the same change.

After checking the live E2E title, the desktop harness sets only `media-tagger.language=en` and `media-tagger.theme=dark`, then reloads before each spec. It leaves unrelated storage untouched. A reload inside a spec retains any language/theme changes, allowing persistence assertions. The tooling tests (`bun run test:e2e-build`) verify that an unexpected or unreadable title ends the worker with status 78 before any preference write, that the launcher's `onWorkerEnd` hook turns that status into a run-ending exit 1, that SIGINT, SIGTERM and SIGHUP end it with 130, 143 and 129, and that a worker exits when its launcher disconnects.

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
The media-kind cases wait until every rendered tile carries the selected kind's chip before reading or opening the first tile, because the previous result set stays rendered until the new query replies. An `afterEach` hook restores English and bulk-off (reload only when needed), so one failed case does not leave Polish selectors for the rest of the suite.
It also checks Polish gallery/settings/lightbox text, both-theme drawers at 600px and 320px, the collapsed floating bulk dock at 900px, native-control appearance, and failure to decode a temporary original. Screenshots capture
the private X11 display into a fresh timestamped `artifacts/ui-redesign/` subdirectory per run.
The normal workflow suite covers narrow lightbox and native media behavior; the separate
`MEDIATAGGER_GALLERY_PERF=1` suite exercises 3,584 images (28 pages, more than the 24-page frontend cache) and asserts that a revisited page is fetched again after eviction. It also checks selection summary IPC and full-selection metadata writes after eviction and filter changes.

Run the opt-in spec through the existing harness on a private X11 display with temporary
XDG data/config/cache directories. Preserve all profile and cleanup guards. The spec's screenshot
guard requires `MEDIATAGGER_NATIVE_DISPLAY` to name that display. Example after setting those
XDG directories:

```sh
unset WAYLAND_DISPLAY; export GDK_BACKEND=x11
dbus-run-session -- xvfb-run -a -s '-screen 0 1440x1100x24 -nolisten tcp' \
  sh -c 'export MEDIATAGGER_NATIVE_DISPLAY="$DISPLAY"; MEDIATAGGER_UI_REDESIGN=1 exec bun run test:e2e:tauri --spec e2e/specs/ui.redesign.e2e.js'
```

### Animation geometry

`e2e/specs/animations.e2e.js` is opt-in with `MEDIATAGGER_ANIMATIONS=1`. It seeds
192 temporary PNGs and long labels, then samples scroll dimensions and positions on
every rendering frame before, during and after interaction. The segmented-selection cases
click through `e2e/native-input.py`, so `XDG_DATA_HOME` must be a fresh `/tmp/mediatagger-*`
directory. Both themes run at 699,
700, 767, 768, 999 and 1000 CSS pixels. Hover/press, bottom-of-gallery selection,
suggestions, standard dialogs, bulk dock, inline information/confirmation and
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
dbus-run-session -- xvfb-run -a -s '-screen 0 1440x1100x24 -nolisten tcp' \
  sh -c 'export MEDIATAGGER_NATIVE_DISPLAY="$DISPLAY"; MEDIATAGGER_ANIMATIONS=1 MEDIATAGGER_REDUCED_MOTION=1 exec bun run test:e2e:tauri --spec e2e/specs/animations.e2e.js'
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

The `ui.redesign.e2e.js` case selected by `--mochaOpts.grep 'corner badges'` checks the Video/GIF badge colors and offsets, floating image/GIF controls down to 320px, and bulk ordering inline in the dock and in the larger modal in both themes. Use `MEDIATAGGER_UI_REDESIGN=1` and the same isolated desktop setup.

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

`e2e/specs/gallery-selection.e2e.js` is opt-in with `MEDIATAGGER_GALLERY_SELECTION=1`. It uses native pointer actions for additive clicks, Ctrl-click toggling, replacing and additive rectangles, shrinking, post-drag click suppression, and edge scrolling in both themes. It also checks that a long press opens the lightbox without changing the selection, Escape clearing and drag cancellation, unused space below a short gallery, and control clicks. A 2,051-file temporary collection proves selection across unloaded pages (about 17 pages, below the 24-page cache, so it does not force eviction; its 2,051-item range is below the 10,000-position limit, so it is resolved to IDs with one binary read); normal-mode clicks still open the lightbox. Screenshots are saved under `artifacts/gallery-selection/`.

Run through the existing isolated harness on a private X11 display with temporary XDG directories:

```sh
MEDIATAGGER_GALLERY_SELECTION=1 bun run test:e2e:tauri --spec e2e/specs/gallery-selection.e2e.js
```

### Timeline and select-all selection spec

`e2e/specs/gallery-timeline.e2e.js` is opt-in with `MEDIATAGGER_GALLERY_TIMELINE=1` (plan stages 08 and 09). It copies 12,005 synthetic PNGs into a temporary root and sets their file times over twelve months of 2023 plus five files in June 2012, then checks the month timeline: labels and `aria-valuemax`, End to the oldest month with its tiles visible and their UTC months read through `get_asset_details`, ArrowUp/Home/PageDown, a native pointer drag that holds demand (`data-scroll-paused`) and settles on the month under the pointer without placeholders, a native pointer click that focuses the rail so the arrow keys move it, and a native click at the right edge of a last-column tile that opens the lightbox (the tile column clears the rail). It then selects all results with Ctrl+A in bulk mode (one range, above the 10,000-position limit), excludes one tile with a native Ctrl-click, adds a tag through one `apply_selection_mutation`, checks that the excluded asset is untagged and that the tag filter returns exactly the selected count, and that the new session clears the range selection with its notice. Key-to-data and tag timings go to `artifacts/gallery-timeline/timings.json`. Use a fresh `/tmp/mediatagger-*` profile as in the other native-input specs:

```sh
MEDIATAGGER_GALLERY_TIMELINE=1 bun run test:e2e:tauri --spec e2e/specs/gallery-timeline.e2e.js
```

### 100k collection: Shift-click and anchored refresh

`e2e/specs/gallery-100k.e2e.js` is opt-in with `MEDIATAGGER_GALLERY_100K=1` (grid-performance stage 10, replacing the temporary spec behind the stage 04 measurements). It copies 100,000 synthetic PNGs (about 450 MB) into a fresh temporary root, scans it through `rescan_all_roots` (polled in the page, because it outlives the WebDriver script timeout) and runs three cases in order:

- In bulk mode, a click near index 50,000 and a native Shift-click 9,999 positions further: exactly 10,000 positions, the largest range that `createSelection` resolves to IDs, so the app must send exactly one `get_asset_query_id_range` and no page reads.
- A click on index 0 and a Shift-click on index 99,999: the counter shows 100,000 and no IDs are resolved (the range stays a selection spec, ADR 0008). The only `get_asset_query_id_range` allowed is the ADR 0029 keepalive read (`start` 0, `end` 0, at most one); any other range read fails the case.
- At index ~50,000, Search without filter changes: exactly one `start_asset_query` whose `anchorOffset` is the first visible index, the anchor's page never requested again (the reply carries it), at most two page reads and only for neighbouring pages that the visible and overscan rows reach, and 240 sampled frames with the same first visible tile and no placeholder in view.

The spec counts commands by wrapping the page's `fetch` for `ipc:` URLs (Tauri's IPC bridge), as `grid-baseline` does. Shift-click times run from the tile's `click` event to the bulk counter text (in-page `performance.now()`) and must stay below 200 ms. Copy, scan and click timings and the refresh evidence go to `artifacts/gallery-100k/timings.json`. Copying takes tens of seconds and the debug scan about as long, so a run takes a few minutes; check the host is idle before reading the timings. Use the isolated recipe with a fresh `/tmp/mediatagger-*` profile:

```sh
MEDIATAGGER_GALLERY_100K=1 bun run test:e2e:tauri --spec e2e/specs/gallery-100k.e2e.js
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
the Mocha suite. The `/audit` QA stages drive it inside a sandbox on a copy of a large dataset;
see the [audit process](audit/README.md) and its [scenario catalog](audit/qa-scenarios.md). The [MediaTagger verification skill](../../.claude/skills/verify-mediatagger/SKILL.md) (`/verify-mediatagger` in Claude Code)
contains the complete launch, doctor, interaction, evidence, and cleanup workflow, with
recipes for gallery/search, settings, and lightbox.

```sh
bun run app:control start
```

Keep `start` running in its terminal or tool exec session. Wait for the `READY` line, then use
its session ID in separate commands. `bun run app:control help` documents the arguments.
The commands are grouped into classes (`COMMAND_CLASS` in `e2e/control-support.js`):

- observe: `doctor`, `inspect`, `screenshot`, `logs`, `wait`, and `alert text`;
- act: `click`, `dblclick`, `modclick`, `fill`, `select`, `keys`, `scroll`, `drag`, `wheel`,
  `alert accept|dismiss|send`, `resize`, and `native`;
- control: `refresh`, `diagnose`, and `stop`.

Commands run one at a time through one WebDriver session. `stop` is handled at once, outside
that queue, so it ends a session even while another command waits on a WebView that does not
answer; queued commands that have not started by then, or whose client already disconnected,
are not run. The 2-second console poll keeps at most one drain queued. Every command except
`stop` and `logs` first runs the doctor, which checks the live E2E title,
resolved app-data path, scan roots, and owned display/driver processes. `validateRequest`
enforces arities and ranges (drag hold 150–3000 ms, wheel ±5000, wait ≤ 90 s, resize
320–1440 × 240–900, native coordinates inside the 1440×900 display, keysyms, printable ASCII).
Modifier keys are always released, and a drag holds at least 150 ms and first moves 6 px, so
it satisfies the rectangle-selection threshold.

The doctor reports a session `state`:

- `ok`;
- `alert-open` when a `window.alert` is showing; WebDriver runs with
  `unhandledPromptBehavior: "ignore"`, and only `alert`, `screenshot`, `doctor`, `logs`, and
  `stop` run until it is handled;
- `busy-maintenance` when the title and app-data path are verified but the `list_scan_roots`
  probe outlasts the 30 s script timeout. The maintenance gate (backup export/import) and the
  scan/thumbnail locks (Clear library, Remove root) wait without a limit, so this lasts as long
  as the operation. Observe commands still run; actions are refused. The guards themselves are
  unchanged;
- `unresponsive` when an identity probe (window title, app-data path) times out. These need the
  app's main thread, which a native modal dialog blocks: GTK's own "The folder contents could
  not be displayed" dialog, shown when a path typed into a save dialog names a missing folder,
  is the known case. Observe commands, `native key Escape` (to dismiss the dialog), and `stop`
  run; every other action is refused. The doctor result names the `probe` that timed out and
  a `hint`;
- `tainted` after a `native` command whose follow-up doctor failed; only `logs`,
  `screenshot`, and `stop` run. A follow-up doctor that reports `busy-maintenance` or
  `unresponsive` does not taint the session.

The WebDriver HTTP request budget (35 s) is 5 s above the 30 s script timeout, so the driver
reports a script timeout before the client aborts the request; a client-side "Request timed
out!" during a probe is classified the same way. One doctor costs at most two request budgets
(the alert probe and the first probe that does not answer), and the 190 s socket budget covers
the doctor before a command plus a 90 s `wait`, or a native command with its 20 s helper limit
and its own doctor (`SOCKET_TIMEOUT_MS` in `e2e/control-support.js`). The `unresponsive` gate
and the out-of-queue `stop` are recorded in [ADR 0023](../adr/0023-app-control-unresponsive-state-and-out-of-queue-stop.md).

`native` runs `e2e/control-native.py` (XTest) with an environment built only from the
session manifest. It refuses before opening any display unless the display is private and
matches the session, `XAUTHORITY` is `<scratch>/Xauthority`, `XDG_DATA_HOME` is
`<scratch>/data`, and `<scratch>/owner` holds the session ID. It drives GTK video controls and
native file dialogs. `key` accepts `Control`/`Ctrl`, `Shift`, `Alt`, `Meta`, and `Super` and maps them to the `_L` keysyms; raw keysyms work too. In a file dialog the recipe is `key Control+l`, `key Control+a`, `type <path>`, `key Return`; without the select-all a save name keeps its `.csv` suffix and becomes `tags.csv.csv`. `fill` selects the field first so it replaces the value, and a bare `*=text` or `=text` selector matches any element by its own text in every command that takes a WebdriverIO selector, not `inspect`, which uses CSS (WebdriverIO would match anchors only). The session scratch root also holds an empty `export/` directory for CSV and backup exports, which the app refuses inside the indexed media root. The controller starts Xvfb on an explicit display from `:90` up (never `:0`/`:1`) and retries the next number if one is taken. The older `e2e/native-input.py` serves the Mocha specs only.

In addition to normal desktop prerequisites, this controller needs Xvfb, xauth, and
ImageMagick's `import`. It creates an authenticated private display with TCP disabled and
uses local driver ports 4446/4447 plus a private Unix socket. Sandbox environments must allow
these local sockets and native processes. An occupied port is refused; the controller does
not attach to another instance. A checkout lock refuses a second controller. Do not run
ordinary desktop E2E or another frontend/E2E build concurrently with it, because build
outputs are shared.

Each session gets fresh XDG data/config/cache directories below a new temporary root
(`media-tagger-control-*` under `realpath(os.tmpdir())`, so `TMPDIR` chooses the parent).
`start` refuses a root whose `control.sock` path would exceed the 107-byte Linux socket path
limit. Cleanup requires the root's parent to equal the controller's `realpath(os.tmpdir())`;
the long-running controller process performs `stop`, so its `TMPDIR` never changes. It
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

`bun run test:privacy` runs `scripts/test_privacy.py` against temporary Git repositories with the real Gitleaks 8.30.1 executable on `PATH`. It verifies historical and staged secrets, private home paths, nested configuration/databases/keys, tracked build output of either Cargo workspace (`src-tauri/target*`, `src-gpui/target*`), shallow-clone rejection and source archives that match a clean commit. `bun run privacy:check` scans this checkout and all reachable history; it is not a scan of ignored working materials.

`bun run test:packaging` also requires Gitleaks because failure/rollback fixtures pass through actual source preparation before their simulated Docker failures. They assert that the intended Docker/export failure was reached. For the real bundle audit and prerequisites, see [release privacy checks](linux-packaging.md#privacy-gate-and-release-inputs).

## Gallery performance measurements

Run the complete opt-in suite in its declared order, using a private Xvfb/D-Bus session and disposable XDG directories as described under [real desktop E2E](#real-desktop-e2e):

```sh
unset WAYLAND_DISPLAY; export GDK_BACKEND=x11
dbus-run-session -- xvfb-run -a -s '-screen 0 1440x1100x24 -nolisten tcp' \
  sh -c 'export MEDIATAGGER_NATIVE_DISPLAY="$DISPLAY"; MEDIATAGGER_GALLERY_PERF=1 exec bun run test:e2e:tauri --spec e2e/specs/gallery.performance.e2e.js'
```

Do not use a single-case filter to verify cache provenance. The timing case must precede the stable-metadata traversal, GIF check and selection scenario. The fixture contains 3,584 PNGs, 12 GIFs and two videos. Library and thumbnail clearing precede indexing; startup/filtering then prepares some previews. Timing uses only images, at indices 128 through 1920 in steps of 128 and then the last index (3583). The same route and filter apply to the revisit. The initial mounted area and untimed returns to 128/0 are excluded from first-use statistics.

`artifacts/gallery-performance/timings.json` records preparation, initial visible/mounted IDs, initial final-JPEG filenames, route/filter, per-scroll file inventories, visible IDs/filenames, raw elapsed times and animation-frame intervals. It classifies samples by whether none, some or all visible JPEGs existed before the scroll. Only the first category enters the first-use summary, and at least one such sample is required. Every revisit sample must reuse existing files. Raw arrays retain order; percentile calculation sorts a copy. File inventories are outside the measured intervals.

The observations measure scroll-to-visible-JPEG-decode with WebDriver/polling overhead. They exclude the subsequent opacity fade and do not flush the OS cache. First-use refers to preview files absent at the pre-scroll snapshot, not uncached source reads or proof that no render was queued. Revisits cross the 24-page frontend cache bound, so they are not promises of resident query pages or decoded images. Keep build profile, viewport, route, fixture and host conditions comparable; this suite has no portable latency threshold. Screenshots and measurements remain ignored local artifacts. See [gallery behavior](../subsystems/library-query-and-gallery.md#gallery-performance-verification) and [thumbnail reuse](../subsystems/thumbnails.md#cache-provenance-in-gallery-measurements).

## Grid-performance baseline

The [grid-performance plan](../plans/grid-performance/README.md) records numbers before and after its stages in the results table of [stage 07](../plans/grid-performance/07-verification-and-docs.md#wyniki). Two opt-in measurements produce them. Neither has thresholds; compare runs made with the same build profile on the same idle host.

`src-tauri/tests/grid_baseline.rs` contains six ignored diagnostics. Run them alone, in release mode, with `TMPDIR` on a disk-backed filesystem, because a tmpfs `/tmp` hides SQLite write costs:

```sh
TMPDIR=/var/tmp cargo test --manifest-path src-tauri/Cargo.toml --locked --release \
  --test grid_baseline -- --ignored --nocapture --test-threads=1
```

- `query_rebuild_and_page_baseline` seeds synthetic 100,000- and 1,000,000-row libraries with SQL. The rows contain kinds, favorites, four-item media groups, 2,000 vocabulary tags, a `common` and a `rare` tag, a scan-root mapping and pseudo-random modification times, 0.1% of them zero. It bumps the revision before each `AssetQueryManager::start` call, so every sample rebuilds the ID vector. The filters are none, favorites, an included tag, a rare tag, an excluded tag and a media kind. It also times cached starts and 128-item pages.
- `mutation_latency_and_rebuilds_during_scan_baseline` times the revision-aware favorite and tag writes through the database runtime, first idle on 100,000 rows. It then repeats them during ten `scan_folder`-shaped scans of 20,000 new small JPEGs. During each scan, a simulated user writes every 50 ms, and a simulated gallery reads a page every 100 ms and rebuilds the session whenever the page is stale. The output reports scan duration, revision bumps, rebuilds, latency distributions and failed writes.
- `sort_key_backfill_plan_and_bulk_group_baseline` times the one-time sort-key backfill of a 1,000,000-row library, prints the plan of the unfiltered order query and times grouping 5,000 assets ([ADR 0004](../adr/0004-trigger-maintained-gallery-sort-keys.md)).
- `ranged_set_media_group_baseline` runs ranged `setMediaGroup` writes through `apply_selection_mutation` on the same synthetic 100,000- and 1,000,000-row libraries: the first tenth of the gallery into a new group, all assets but three into one group, the same again (no row changes), and every group cleared. It reports the processed and updated counts, the call time, and the longest stretch another connection polling `BEGIN IMMEDIATE` every 200 µs was locked out ([ADR 0026](../adr/0026-set-based-ranged-group-write-with-index-rebuild.md)).
- `jpeg_24mp_thumbnail_baseline` generates a 6000×4000 JPEG and times `thumbs::create_image_thumb`.
- `steady_scroll_visible_thumbnail_wait_baseline` models a gallery of 8 columns, 5 visible rows and 4 overscan rows that scrolls 160 rows at 20 rows/s over never-rendered images and then stops. The real scheduler renders hard links of one synthetic 24 MP JPEG with this host's browsing worker count and with 2 workers. For sticky prefetches (before [ADR 0021](../adr/0021-visible-tiles-drop-a-pending-prefetch.md)), a LIFO High queue (modelled as Interactive) and the `p=1` drop, it reports the backlog at the stop and how long each tile visible at the stop waits for its result. The request pattern is modelled; WebKit and IPC are not part of it.

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
| GPUI port: client model, text or theme logic | Rust unit test in `media_tagger_client` | `#[gpui::test]` in `media_tagger_gpui` when layout or input handling is part of the claim |
| GPUI port: view, window, profile or startup composition | Rust test in `media_tagger_gpui` (`TestAppContext`, temporary profile) | The headless renderer for pixels; the real binary on a private Xvfb display for the window manager side |

## Known limitations

- jsdom and mocked Tauri calls do not prove IPC serialization or native runtime behavior.
- `backend_e2e.rs` does not test the desktop, command serialization, the native CSV dialog/parser path, or profile isolation.
- The desktop suite is an unbundled debug build and does not verify the release artifact's exact shared-library environment.
- Desktop E2E requires a configured Linux host. The release container does not include WebKitWebDriver, Xvfb, or `tauri-driver`.
- Specs start in English/dark after the title guard; translated scenarios must switch language explicitly.
- An `EPERM` during E2E app-data deletion leaves stale isolated data and does not fail fast.
- Desktop runs share `dist/`, the E2E target and driver port. Run them serially, including standalone E2E builds.
- `compare:ui` shows that both applications were captured in the named state; it does not compare pixels and has no threshold. Its GPUI session has no input commands, and no automated test starts a real session (see [comparison harness](#comparison-harness-tauri-and-gpui)).
- Native video playback depends on libmpv, GTK, and OpenGL. WebDriver sees the native session container and backend snapshot attributes, but not decoded `GtkGLArea` pixels or the native GTK control overlay.
- A private `XDG_RUNTIME_DIR` has no PipeWire or PulseAudio socket, so desktop runs that follow the documented recipe play video through libmpv's null audio output. No suite verifies audible output, and no suite measures frame pacing or A/V sync.
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
`adapter/startup_scan.rs` cover launch-snapshot contention, shared waiting,
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

`backend_runtime.rs` exercises precise fingerprint/version coupling, version-2 failure migration, dedicated FULL-synchronous connections, and a real 514-file scan across batching and overlapping roots. Unit tests cover maintenance admission/draining, pool callback cleanup, query cache budget/epoch, stale `thumb://` renders that are never served and re-render on retry, directory-sync failures after successful moves, disk-staged CSV row order/clears, and bounded subprocess diagnostics/timeouts.

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

Lower-layer regressions are `src/features/lightbox/__tests__/createLightbox.test.ts`, `literalTagSearch.test.ts`, the library model/API tests, the query-position service test, and `src-tauri/tests/thumbnail_clear.rs`. The Rust file-backed test also checks nested obsolete files, skipped symlinks, exclusive-reader coordination, failure-marker removal, source preservation, actual image regeneration, and the two-pass sweep's progress when files appear or vanish between the counting and deleting walks.

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
controlled exclusion updates and keyboard focus. `BulkDock.test.tsx` uses
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
Evidence remains under `artifacts/tag-editor/<timestamp>/`. The popup assertion is
placement-aware: the lightbox editor opens suggestions below the input, and the bulk
dock (at the bottom of the viewport) opens them above it. The bottom-edge placement
limitation of below-input lists remains a separate issue; the lightbox check uses a
desktop viewport with room below the input.

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
explicit batch boundaries. Virtual grid tests retain 10,000/26,000/1,000,000-slot limits,
row-pool identity, null-ref startup, resize anchoring and cleanup. Query/cache tests
exercise page eviction and in-place page merges. Model tests use deferred IPC for
canonical publication, contention, partial responses and disposal.

Run opt-in suites under the private display and disposable XDG profile described above:

```sh
MEDIATAGGER_GALLERY_BROWSING=1 bun run test:e2e:tauri --spec e2e/specs/gallery-browsing.e2e.js
MEDIATAGGER_TAG_EDITOR=1 MEDIATAGGER_GALLERY_SELECTION=1 bun run test:e2e:tauri \
  --spec e2e/specs/tag-editor.e2e.js --spec e2e/specs/gallery-selection.e2e.js --spec e2e/specs/group-order.e2e.js
MEDIATAGGER_SETTINGS_MAINTENANCE=1 bun run test:e2e:tauri --spec e2e/specs/settings-maintenance.e2e.js
```

Gallery browsing indexes 2,048 PNGs plus GIF/video fixtures, crosses 16 pages (within the
24-page cache, so eviction is covered by `gallery.performance`), keeps the DOM bounded, and records geometry at 1440/600/320px in English/Polish and both themes.
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

The spec indexes 2,052 copied PNG fixtures and scrolls across many query pages, so the
selected range spans unloaded and evicted pages. Selections read IDs through the binary
`get_asset_query_id_range` command, not pages: the spec fails that read by sending a
negative unsigned `start` at the fetch boundary. Native Shift-click and rectangle
selection must retain their previous IDs, render the gallery error banner, survive its
actual ResizeObserver geometry change, and complete the original additive range through
the visible Retry button with exactly one successful range read for `0..=endpoint`. It
records range requests and measured grid margins under ignored `artifacts/selection-recovery/`.
The scroll helper allows one pixel at viewport edges because WebKit rounds `scrollTop`
while active rectangle scrolling preserves fractional offsets. A failure records the
last forty scroll attempts, including requested index, actual offsets and tile bounds.
Frontend regressions in `GalleryGrid.test.tsx`, `createGridHandlers.test.tsx` and
`createSelection.test.ts` cover the integrated query/selection/banner path, pending reads,
live gesture cancellation, new input, mode/query changes, identity reset and disposal.


## Million-result gallery layout

Run `sh e2e/gallery-scale/run.sh` from the repository root. It builds the actual
`GalleryGrid`, its row-pool virtualizer, selection model and production CSS into a standalone
fixture, then opens an ephemeral WebKitGTK 4.1 view on a private Xvfb display.
Python needs PyGObject and GTK 3/WebKit2 4.1 typelibs. Native input uses libX11 and
libXtst; screenshots require ImageMagick `import`. The runner uses temporary XDG
folders, a loopback fixture server and its own display. It does not attach to Tauri
or read the user's library. Do not run `webkit.py` against a normal desktop.

Lazy synthetic summaries exercise one million results at one, five, six and eight
columns, narrow width, tile-size changes, viewport-height changes and resize anchors.
Assertions require final index 999999 to be visible, demanded and selected by native
clicks, with a spacer no taller than `MAX_GRID_HEIGHT` (16,000,000 pixels), the windowed
mode with the gallery scrollbar, unchanged columns when the native gutter is removed, and
fewer than 120 mounted tiles. XTest drags the gallery scrollbar thumb, presses its track,
sends End/Home from a focused tile, wheels after an idle window recentre (which must keep
the visible tiles), and drives Shift-click and rectangle selection. The fixture checks
global selection IDs and range reads, then disposes the gallery and checks that further
scroll/resize events publish no ranges. It also includes a native 100-result control. Screenshots, JSON results and profile/server
cleanup evidence remain ignored under `artifacts/gallery-scale/`.

This establishes native layout and input behavior with synthetic data. Range callbacks
represent page demand; there is no million-row SQLite query, thumbnail rendering or
performance comparison. The integrated `GalleryGrid.test.tsx` regression separately
runs the real sparse page cache with a mocked transport, demands offset 999936 and
checks the final tile, bounded cache and footer total. Footer tests check the
end state and its language change; there is no sequential `hasMore`/reach-end loader, so
sparse viewport demand is the only page loader. For live media/IPC and timing provenance
use the separate gallery browsing and performance suites above.

`bun run typecheck` and `bun run lint` cover the fixture sources (`e2e/gallery-scale/tsconfig.json`, `*.ts`/`*.tsx`), so there is no separate manual `tsc` step.

```sh
bun run test -- src/features/gallery src/app/__tests__/gallery.test.tsx src/library/__tests__/createLibrary.test.ts src/library/query/__tests__/createAssetQuery.test.ts
sh e2e/gallery-scale/run.sh
```

### Million-result timeline jump and select-all

`GALLERY_MILLION=1 sh e2e/gallery-scale/run.sh` opens `million.html` (`million.tsx`) in the same private WebKitGTK setup. It mounts the production `createLibrary` page cache, `GalleryGrid` with `GalleryTimelineScrubber`, and `createSelection` wired as in `createAppRuntime`. Only the transport is synthetic: one million results, 120 months (January 2016 to December 2025) split into backend-sized sections of at most 5,000, and pages and the sections read answered after 12 ms. A stand-in `<output>` shows `selection.count`, as the bulk dock counter does.

- **Timeline:** three samples, each from a fresh session (empty page cache). XTest presses End on the focused rail; the time runs from the first capturing `keydown` listener to the first animation frame in which the first tiles of the oldest month are in view with data. Each End sample must request at least one page (a sample served from cache fails the run; the page count is recorded but not compared with one) and stay below 300 ms; Home returns to the anchor page. The rail's `aria-valuetext` must read the oldest month.
- **Select-all:** in bulk mode, XTest Ctrl+A from a focused tile; the time runs from the `keydown` listener to the counter text `1000000` (MutationObserver) and must stay below 50 ms, with no `get_asset_query_id_range` call (the mock rejects it), no materialized IDs (`ids.size === 0`, `ranged`), and visible tiles marked selected. Three samples.

WebKitGTK's `event.timeStamp` for XTest keys is not on the `performance.now()` clock, so the fixture timestamps in its own listener; the native input latency before that is not part of the number. Results go to `artifacts/gallery-scale/result.json`, with screenshots `million-oldest-month.png` and `million-select-all.png`. Like the other fixtures this measures frontend work on software-rendered Xvfb, not SQLite, real thumbnails or native IPC; the backend side of both operations is in `grid_baseline.rs` and `backend_integration` (`one_million_select_all_tag_and_favorite`).

```sh
GALLERY_MILLION=1 sh e2e/gallery-scale/run.sh
```

### Scroll height probe

`GALLERY_HEIGHT_PROBE=1 sh e2e/gallery-scale/run.sh` opens `height-probe.html`, a plain-DOM
page with the production `.gallery-scroll` CSS, in the same private WebKitGTK setup. For
spacers from 4M to 33.5M pixels it records `scrollHeight`, `scrollTop` write/read-back
(odd and fractional), the layout offset and `elementFromPoint` hit of a 2D-translated
`contain: strict` marker near the end, one native wheel step, and a screenshot per height.
It then compares the native gutter with `scrollbar-color: transparent` and
`scrollbar-width: none`. Check painting afterwards with ImageMagick, for example
`convert artifacts/gallery-scale/height-probe-paint-16000k.png -format '%[pixel:p{150,290}]' info:`
(the marker is `srgb(204,51,51)`). For device scale 2, run `webkit.py` through the same
environment with `GDK_SCALE=2` and a larger Xvfb screen, and double the pixel coordinates.
WebKitGTK 2.54.2 was exact up to 16,777,000 pixels and from 16,800,000 pixels rounded odd
offsets and stopped painting the marker at both scales; `MAX_GRID_HEIGHT` derives from this
(see [gallery behavior](../subsystems/library-query-and-gallery.md#virtual-range-loading-and-thumbnail-prefetch)).

## Fast gallery scrolling

`createVirtualGrid.test.tsx` covers the half-viewport threshold, cumulative logical
distance across reversals, slow motion, the 150 ms idle deadline, unchanged final
ranges, geometry anchors and disposal. The integrated
`GalleryGrid.test.tsx` checks that a fast jump keeps loaded tiles and images
while page and thumbnail demand stop, that paused placeholders do not
pulse, and that uncached slots skip passing pages and resume the final range.
Library tests retain late pages and independent selection/lightbox reads through
pause and filter changes.

Run the gallery and library suite, type checks and native scale fixture with the
commands in [million-result gallery layout](#million-result-gallery-layout).
The native fixture waits past the pause deadline before inspecting loaded tiles.

For a separate native timing run:

```sh
GALLERY_PERFORMANCE=1 sh e2e/gallery-scale/run.sh
```

`performance.tsx` uses the actual gallery and sparse page cache with `thumb://` tiles,
26,000 synthetic summaries and generated SVG thumbnails. The Tauri transport is mocked
with 12 ms page replies; `webkit.py` registers a synthetic `thumb` scheme that answers
each thumbnail after 20 ms. XTest drags the real
scrollbar thumb in sixty steps spaced 20 ms apart, repeating three times after
query/cache reset. Both variants must reach index 25999. The script records rAF
intervals during motion and the first 150 ms after the last position change,
page invocation counts, `thumb://` and cached-only (`c=1`) image requests, and time from the last
scroll event to a loaded visible image and to image opacity above 0.95. It also
checks that no paused frame has pulsing placeholders or spinners, and records paused
frames that still show loaded images and the tile elements created per drag.

For a before/after comparison, run the fixture on the earlier commit and on the change,
copying `artifacts/gallery-scale/result.json` after each run because the next run
replaces it. (The former `GALLERY_BASELINE_DIR` source substitution was removed with
`@tanstack/virtual-core`, which the old sources need.) Run both variants without
concurrent builds or test suites. This is a frontend
comparison on software-rendered Xvfb, not a SQLite, native IPC or real-media decode
benchmark. Results include the pause's deliberate delay. Gallery images now appear at full opacity
as soon as they load; the historical comparison below was recorded with the fade enabled.

The runner leaves raw samples, request traces, screenshots and profile/server
cleanup evidence in ignored `artifacts/gallery-scale/`. The performance screenshot
is `performance-thumbnails.png`.

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
sequential near-end callback. That callback has since been removed entirely (grid
performance stage 04): only viewport demand loads pages, and the integrated
million-result regression rejects earlier offsets. The timing fixture no longer passes a
near-end callback.
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
