# Solid 2 migration progress

## Current state

Stage 2 is complete. React remains the default and functional reference.
Solid now has the platform boundary, window shell, Appearance/Language settings, shared
controls, modal ownership and lazy loading boundaries. Feature workflows follow in stage 3.

- Working branch: `feature/rewrite-react-to-solid2`.
- React reference commit: `f11126dfafd40f877ecf5e496abc37d77a435e78`.
- Stage 1 starting commit: `0e1cc02`, the migration plan.
- Next implementation task: [stage 3](03-library-state.md).
- No worktrees, GitHub pushes, or merges to `main` are permitted.

## Stage tracker

Statuses: `Not started`, `In progress`, `Blocked`, `Ready to commit`, `Complete`.
A stage becomes `Complete` only after its tests, self-review, fixes, and local commit.
Use this table as the status source of truth; the stage files describe planned work.

| Stage | Status | Tests | Self-review | Local commit |
| --- | --- | --- | --- | --- |
| [1. Reference and toolchains](01-reference-and-toolchains.md) | Complete | 703 frontend, 206 Rust, both builds, desktop and packaging verified | Findings fixed and rechecked | `build: add isolated Solid 2 migration setup`; `4af0ff4` |
| [2. Platform and shell](02-platform-and-shell.md) | Complete | 734 frontend, 206 Rust, both builds, desktop shell comparison | Findings fixed and rechecked | `refactor: establish Solid shell and platform boundaries` |
| [3. Library state](03-library-state.md) | Not started | Not run | Pending | None |
| [4. Gallery and search](04-gallery-and-search.md) | Not started | Not run | Pending | None |
| [5. Tagging and bulk](05-tagging-and-bulk.md) | Not started | Not run | Pending | None |
| [6. Lightbox and images](06-lightbox-and-images.md) | Not started | Not run | Pending | None |
| [7. Native video](07-native-video.md) | Not started | Not run | Pending | None |
| [8. Settings and maintenance](08-settings-and-maintenance.md) | Not started | Not run | Pending | None |
| [9. Parity and cutover](09-parity-and-cutover.md) | Not started | Not run | Pending | None |
| [10. Documentation](10-documentation.md) | Not started | Not run | Pending | None |

## Planning baseline evidence

These checks ran against the unchanged React application during planning on 4 October
2026. They do not constitute Solid verification or completion of stage 1.

| Check | Result | Limits |
| --- | --- | --- |
| `bun run test` | Passed: 84 files, 699 tests | Existing React frontend and pure tests only |
| `bun run typecheck` | Passed | Existing application and configuration types only |
| Public npm metadata inspection | RC/tooling versions recorded in the README | Recheck before installation; compatibility not yet exercised locally |
| Rust tests | Not run during planning | Required future checks remain outstanding |
| Desktop E2E | Not run during planning | No visual, native playback, or desktop parity claim |
| Solid installation/build/tests | Not performed | Planning session contained no implementation |

## Updating this file

For each stage, record the changed behavior, exact verification commands, results,
review findings and fixes, remaining limitations, and the local commit reference.
Link to ignored local artifacts only when they contain synthetic data. Do not paste
private paths, media metadata, databases, credentials, or raw logs into tracked files.

Write the evidence before the stage commit. Fill in a completed stage's hash when
starting the next stage. For the final stage, retain its exact commit subject here and
resolve the hash through Git history rather than making a commit that tries to include
its own hash.

Do not mark a required skipped or unavailable check as passed. A blocked stage should
name the missing prerequisite and the remaining verification without claiming completion.

## Stage evidence template

Copy this section when an implementation stage starts:

```text
Stage:
Status:
Date:
Starting commit:

Implemented behavior:
-

Tests written, moved, or replaced:
- Existing suite/assertion -> new owner and equivalent behavior

Verification:
- Command:
  Result:
  Evidence:

Self-review:
- Finding:
  Fix:
  Reverification:

Remaining limits or blockers:
-

Local commit subject:
Local commit hash, recorded after commit:
Next stage:
```

## Stage 1 evidence

Date: 4 October 2026. Starting commit: `0e1cc02`.
Local commit subject: `build: add isolated Solid 2 migration setup`.
Local commit hash: `4af0ff4`.

Implemented behavior:

- Exact Solid runtime/compiler/plugin/testing pins, rechecked against public npm metadata.
- React remains the default. `MEDIATAGGER_FRONTEND=solid` selects only the Solid entry and
  compiler; unsupported values fail. Both entries retain `index.html`, `dist/`, and port 1420.
- Separate React/Solid TypeScript contexts and React/Solid/shared Vitest projects.
  Shared mocks remain common; React language setup is awaited and Solid roots are disposed.
- The E2E build helper builds fresh selected assets before native packaging. The WDIO
  harness runs the Solid bootstrap spec for Solid, and exits if preparation fails.
  Existing identifiers, target paths, native-title checks, and cleanup guards remain intact.
- Flatpak source generation supports the reviewed prerelease cache names, including the
  compiler's transitive `gensync` dependency. Unknown prereleases still fail.
- [The behavior inventory](BEHAVIOR-INVENTORY.md) assigns all 84 original frontend suites.
  All 699 original frontend tests and their assertions are retained unchanged.

Package baseline and installed toolchain:

| Package/tool | Version |
| --- | --- |
| React / React DOM | 18.3.1 |
| TanStack React Virtual / resolved virtual-core | 3.14.10 / 3.17.8 |
| Vite / Vitest / TypeScript | 8.2.2 / 4.1.11 / 5.9.3 |
| Solid / @solidjs/web / @solidjs/compiler | 2.0.0-rc.13 |
| @solidjs/vite-plugin | 3.0.0-next.47 |
| @solidjs/testing-library | 1.0.0-beta.3 |
| Verification host Node / Bun | 26.7.0 / 1.4.0 |

The full original package graph remains reproducible from the reference commit and
`bun.lock`. Node's minimum is now `>=22.12.0`; the repository and packaging Node pin remains
22.22.0. The old runtime packages retain their resolved versions; Bun relocated some
transitive lockfile entries when adding Babel's dependencies.

Tests added:

- Solid bootstrap rendering and explicit browser-runtime assertion, user-event reactive
  update, deferred completion, owned listener cleanup, and automatic root cleanup.
- E2E build order and selection, rejection of invalid selection/profile, and termination
  before workers on build failure, using fake tools in a disposable project.
- Real pinned-Bun frozen offline installation from generated prerelease cache directories.
- Native Solid mount/reload smoke and a React metadata-loading capture with a held real
  response. The latter restores the fetch function and releases the pending response.

Verification:

| Command | Result | Local evidence |
| --- | --- | --- |
| `bun run test` before edits | 84 files, 699 passed | `artifacts/solid2/stage1/baseline-frontend.log` |
| `bun run typecheck` before edits | Passed | `artifacts/solid2/stage1/baseline-types.log` |
| `bun run test` after edits | 85 files, 703 passed | `artifacts/solid2/stage1/frontend.log` |
| `node node_modules/vitest/vitest.mjs list --json` | 577 React, 122 shared, 4 Solid; inventory matches, no duplicate suite assignment | `artifacts/solid2/stage1/discovery.json` |
| `bun run typecheck` | All three contexts pass | `artifacts/solid2/stage1/types.log` |
| `bun run test:frontends` | Both builds pass, selected entry present, other renderer absent | `artifacts/solid2/stage1/builds.log`, `react-modules.json`, `solid-modules.json` |
| `bun run test:backend` | 206 passed, 2 diagnostic tests ignored | `artifacts/solid2/stage1/backend.log` |
| `bun run test:app-control` | 6 passed | `artifacts/solid2/stage1/control.log` |
| `bun run test:e2e-build` | 3 passed | `artifacts/solid2/stage1/e2e-build.log` |
| `PATH="/tmp/solid2-tools:$PATH" bun run test:packaging` | 18 passed, including offline prerelease installation | `artifacts/solid2/stage1/packaging.log` |
| `bun run lint` / `bun run format:check` / `bun run locale:check` | Passed; 11 locales valid | `artifacts/solid2/stage1/{lint,format,locales}.log` |
| `MEDIATAGGER_FRONTEND=invalid bun run dev` | Expected rejection before serving | `artifacts/solid2/stage1/invalid-dev.log` |
| `MEDIATAGGER_FRONTEND=invalid bun run test:e2e:tauri` | Expected rejection before desktop preparation | `artifacts/solid2/stage1/invalid-desktop.log` |

Self-review findings and fixes:

- A standalone E2E build previously consumed arbitrary existing `dist/`. Fresh frontend
  compilation now lives in the shared build helper; duplicate caller builds were removed.
- WDIO logs thrown preparation errors and continues. The harness now exits on any
  preparation failure; a subprocess regression proves workers cannot follow a failed build.
- The first broad desktop run exposed an obsolete horizontal-language-layout assertion.
  The existing React component stacks the field at all widths; the test now checks that
  behavior. The suite also restores English and desktop size so startup-scan selectors
  cannot inherit Polish. Both corrected scenarios passed in the full rerun.
- A visual hit-target probe sometimes ran after the three-second lightbox idle timeout.
  The test now supplies keyboard activity before checking clickability, preserving the
  original geometry and hit-target assertions. Application idle behavior is unchanged.
- Packaging rejected prerelease cache names. The bounded mapping matches the actual pinned
  Bun cache and is exercised through real offline installation. Gitleaks was initially
  missing from PATH; the repository's checksum-pinned installer supplied it under `/tmp`.
- Shared-test discovery is limited to existing reference directories so future Solid
  service suites cannot accidentally run in both shared and Solid projects.

Desktop evidence:

The local `artifacts/solid2/stage1/run-desktop.sh` creates fresh XDG directories under
`/tmp`, unsets Wayland, and runs the normal WDIO command under a private Xvfb and D-Bus
session. It exports the owned display for native-input scenarios. These commands ran
serially; the profiles were removed after the sessions exited.

```sh
MEDIATAGGER_FRONTEND=react MEDIATAGGER_UI_REDESIGN=1 MEDIATAGGER_GALLERY_PERF=1 MEDIATAGGER_TAG_EDITOR=1 MEDIATAGGER_SETTINGS_LANGUAGE=1 MEDIATAGGER_SOLID_REFERENCE=1 bash /tmp/solid2-desktop.sh
MEDIATAGGER_FRONTEND=react MEDIATAGGER_UI_REDESIGN=1 bash /tmp/solid2-desktop.sh --spec e2e/specs/ui.redesign.e2e.js --mochaOpts.grep 'shows corner badges'
MEDIATAGGER_FRONTEND=solid bash /tmp/solid2-desktop.sh
```

- The broad React rerun exercised 50 cases, with 49 passes and the idle-timer test failure
  described above. It skipped 57 cases behind other opt-in flags, including Solid's smoke.
  The corrected visual case then passed in a fresh focused run. The broad run's exit code
  remains 1 in its preserved log; it is not represented as a fully green invocation.
- Solid's native mount/reload test passed with exit 0. React remains the functional UI.
- Logs are `desktop-react-final.log`, `desktop-visual-recheck.log`, and `desktop-solid.log`
  under `artifacts/solid2/stage1/`. The earlier failed baseline run is `desktop-react.log`.
- `artifacts/ui-redesign/2026-10-04T08-38-10-697Z/` contains both themes, English/Polish,
  320/600/1000/1440px layouts, empty/filtered results, dialogs and unreadable-original UI.
  The focused recheck added `artifacts/ui-redesign/2026-10-04T08-42-07-180Z/`.
- Sixteen footer measurements keep the last tile above the footer and the footer inside
  the viewport. At 1000px in the light English gallery, the last tile bottom and footer
  top are both 583px; footer bottom is 720px and height is 137px.
- `artifacts/gallery-performance/` records cache/selection behavior across 2,048 images.
  The same-host diagnostic p95 samples were 49ms cold / 37ms warm frame intervals and
  344ms cold / 161ms warm thumbnail completion. These timings are observations, not new
  acceptance thresholds.
- Inspected the actual English/light and Polish/dark gallery screenshots, metadata-loading
  capture, unreadable-original error, and Solid bootstrap pixels. All use synthetic media.
- `bun run build` restored default React assets after Solid verification. `bun run audit:js`
  reported no vulnerabilities across 24 production packages.
- `PATH="/tmp/solid2-tools:$PATH" bun run privacy:check` passed across 76 prior commits,
  3,554 complete objects and tracked working files. Its log is `artifacts/solid2/stage1/privacy.log`.

Final review covered compiler/type isolation, discovery, module graphs, root cleanup,
E2E failure handling, pinned offline-cache behavior, and documentation accuracy. No React
component behavior, native profile, IPC payload, or backend implementation changed.
Staged content was reviewed for scope and privacy before the local commit; screenshots,
raw logs, dependency graphs, databases and media remain ignored.

Remaining limits: no full Flatpak/AppImage rebuild or installed-package smoke, no new
cross-platform desktop claim, and no Solid feature parity claim. Native video speed,
reduced-motion/animation, selection-drag, thumbnail-fade, About, and activity opt-in suites
were not enabled for this bootstrap change. The existing default native workflow tests
passed; native video pixel quality is not established by them. Two Rust diagnostic tests
remain ignored as in the baseline. There are no outstanding required stage-1 checks.

Next stage: [platform boundaries and Solid shell](02-platform-and-shell.md).

## Stage 2 evidence

Date: 4 October 2026. Starting commit: `4af0ff4`.
Local commit subject: `refactor: establish Solid shell and platform boundaries`.
Record its hash when opening stage 3.

Implemented behavior:

- Split all 45 API functions and transport declarations into `src/platform/tauri` subsystem
  modules. Kept React compatibility exports and moved video contracts out of the lightbox.
  `SelectedAsset` remains view state. Every production Tauri import now enters through the
  platform directory, including events, dialogs, the system opener and native windows.
- Split shared i18next setup/resources/persistence from its React binding. Solid owns one
  language subscription per application tree, with live translation and disposal. All 11
  languages, prefix normalization, fallback and storage/document behavior remain intact.
- Added the Solid shell/runtime and settings navigation for Appearance and Language. Theme
  applies before rendering and syncs to the native window. Bootstrap owns context-menu and
  optional performance listeners, cancels late mounting on disposal, and supports HMR.
- Ported window controls, buttons, icons, chips, alerts, progress, sliders and segmented
  radios with existing markup/classes and controlled updates. Native window reads discard
  stale completions and late listener registration unlistens after disposal.
- Added portal layers, nested Escape ordering, focus wrapping/restoration, inert background,
  scroll locking, retained exit presence and live reduced motion. Closing releases modality
  immediately. `createModalSnapshot` lets future feature models retain cleared draft values
  during exit; mounted Solid children continue reacting unless their values use that helper.
- Added settings loading/failure recovery and the lightbox loading/error boundary for stage
  6's media view. Settings and the lightbox boundary prefetch after 1,500 ms. Cleanup cancels
  the timer; rejected chunks retain reload/Back/Close presentations.
- Shared framework-independent theme, modal animation and segmented-highlight measurement
  with React. Corrected owning architecture, IPC, localization and test documentation.

Tests moved or added:

- API and theme suites moved with their assertions intact. All 699 reference tests remain.
- Solid has 35 tests, including the four stage-1 bootstrap checks. New coverage exercises
  window actions, late registration and stale reads; reactive control attributes and icons;
  radio keyboard/geometry behavior; rejected controlled input; nested/locked dialogs;
  focus and root-attribute restoration; exit snapshots, cancellation and reduced motion;
  lazy success/failure/reset; all languages; theme persistence; settings remounts; context
  menus, delayed prefetch and performance-observer cleanup.
- React presence/StrictMode tests still run against the shared animation helper. Solid
  verifies ownership and disposal directly. Feature-specific confirmation tests remain
  with React until their planned migration stages.
- Desktop shell comparisons now run serially for either selected renderer using the
  existing isolated harness. They cover both themes, English/Polish, three widths,
  persisted reloads, native control geometry and Escape focus restoration.

Verification, with evidence under `artifacts/solid2/stage2/`:

| Command | Result | Evidence |
| --- | --- | --- |
| `bun run test` | 89 files, 734 passed | `frontend-final.log` |
| `node node_modules/vitest/vitest.mjs list --json` | 577 React, 122 shared, 35 Solid; no duplicated moved suites | `discovery.json` |
| `bun run test --project solid` | 5 files, 35 passed | `solid-final.log` |
| `bun run typecheck` | All three contexts pass | `types.log` |
| `bun run test:frontends` | Both builds and renderer/module-entry separation pass; leaves React assets in `dist` | `builds-final.log`, `react-modules.json`, `solid-modules.json` |
| `bun run test:backend` | 206 passed, 2 pre-existing diagnostic tests ignored | `backend.log` |
| `bun run lint` / `bun run format:check` / `bun run locale:check` | Passed; all 11 locales valid | `lint.log`, `format.log`, `locales.log` |
| `python3 artifacts/solid2/stage2/check-extraction.py` | 45 API signatures/bodies and 48 declarations match stage 1 after whitespace/leading-union normalization | `extraction.log` |
| `python3 artifacts/solid2/stage2/compare-controls.py` | All 12 React/Solid samples match for dimensions, fonts, colors, radius, viewport fit and persisted values | `control-parity.log` |
| `PATH="/tmp/solid2-tools:$PATH" bun run privacy:check` | Passed for existing history and tracked working files; repeated after staging | `privacy.log`, `privacy-staged.log` |
| `git diff --check` | Passed | Working/staged diff review |

Desktop commands use a private Xvfb/D-Bus session and fresh XDG directories under `/tmp`.
The ignored wrapper is `artifacts/solid2/stage2/run-desktop.sh`.

```sh
MEDIATAGGER_FRONTEND=solid MEDIATAGGER_SOLID_SHELL=1 bash artifacts/solid2/stage2/run-desktop.sh
MEDIATAGGER_FRONTEND=react MEDIATAGGER_SOLID_SHELL=1 bash artifacts/solid2/stage2/run-desktop.sh --spec e2e/specs/solid-bootstrap.e2e.js
```

Solid passed 13 desktop cases. React passed the 12 shared cases and skipped the Solid-only
bootstrap case. Final logs are `desktop-solid-final.log` and `desktop-react-final.log`.
Screenshots and measurements are in `solid/` and `react/`. Inspected actual English/light
and Polish/dark Solid pixels, including 320px, and the React English/light reference.
Only shell controls are compared: Solid intentionally has no library results or remaining
settings sections yet. Screenshot positions differ where React contains those sections.

Self-review findings and fixes:

- Solid 2 disallows `onCleanup` inside `onSettled`. Native window registration now returns
  its cleanup directly; the late-registration test verifies it still releases listeners.
- Native radio and range inputs can change without the parent accepting a value. Controls
  restore the committed value while awaiting the batched parent update; tests cover both
  rejected and accepted input, external updates, and native radio keyboard behavior.
- Overlapping native window reads could publish an older maximized/fullscreen result.
  A synchronous generation guard rejects that completion; a deferred-promise test proves it.
- Animation and highlight measurement were initially duplicated. Both renderers now call
  shared DOM helpers. The complete frontend suite passed after extraction.
- The first React desktop comparison had 4 passes and 8 failures because WebDriver scrolled
  the narrow reference navigation beneath its sticky header. The test now returns the main
  scroller to the top before clicking section links. Application navigation is unchanged.
  `desktop-react-initial.log` retains the failed run; the corrected comparison passed.
- The initial sandbox could not bind the private D-Bus socket. Desktop checks then ran with
  approved local socket access. Early wrappers hit portal FUSE mounts during cleanup after
  successful WDIO runs. The wrapper now retries removal after session exit. Old temporary
  profiles were removed separately; final runs check cleanup as part of their exit status.

Review covered the actual diff, unchanged command serialization, React compatibility,
reactive props and event behavior, ownership, late completion, focus/layer cleanup, module
boundaries, test discovery and compiled renderer graphs. Staged content was reviewed for
scope and privacy before the local commit; raw logs, screenshots and runtime profiles
remain untracked.

Remaining limits: no full feature parity, native-video playback or cross-platform claim;
no packaging rebuild for this frontend-only stage. The original two Rust diagnostics remain
ignored. Shared library state starts at stage 3, gallery/search at stage 4, lightbox media
at stage 6, and remaining settings workflows at stage 8.

Next stage: [shared library state](03-library-state.md).
