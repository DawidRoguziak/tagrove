# Solid 2 migration progress

## Current state

Stage 9 is complete. Solid is the normal entry and React has been removed after
parity verification. Final Solid-only checks and review pass.
The canonical documentation rewrite is stage 10.

- Working branch: `feature/rewrite-react-to-solid2`.
- React reference commit: `f11126dfafd40f877ecf5e496abc37d77a435e78`.
- Stage 1 starting commit: `0e1cc02`, the migration plan.
- Next implementation task: [stage 10](10-documentation.md).
- No worktrees, GitHub pushes, or merges to `main` are permitted.

## Stage tracker

Statuses: `Not started`, `In progress`, `Blocked`, `Ready to commit`, `Complete`.
A stage becomes `Complete` only after its tests, self-review, fixes, and local commit.
Use this table as the status source of truth; the stage files describe planned work.

| Stage | Status | Tests | Self-review | Local commit |
| --- | --- | --- | --- | --- |
| [1. Reference and toolchains](01-reference-and-toolchains.md) | Complete | 703 frontend, 206 Rust, both builds, desktop and packaging verified | Findings fixed and rechecked | `build: add isolated Solid 2 migration setup`; `4af0ff4` |
| [2. Platform and shell](02-platform-and-shell.md) | Complete | 734 frontend, 206 Rust, both builds, desktop shell comparison | Findings fixed and rechecked | `refactor: establish Solid shell and platform boundaries`; `aa7c933` |
| [3. Library state](03-library-state.md) | Complete | 822 frontend, 206 Rust, both builds; query follow-up passed | Findings fixed and rechecked | `feat: implement Solid library state and mutation coordination`; `64e7ac1` |
| [4. Gallery and search](04-gallery-and-search.md) | Complete | 931 frontend, 206 Rust, both builds; desktop comparison and reduced motion passed | Findings fixed and rechecked | `feat: migrate gallery and search to Solid`; `9a87d96` |
| [5. Tagging and bulk](05-tagging-and-bulk.md) | Complete | 1008 frontend, 206 Rust, both builds; 7 desktop cases per renderer and matching editor geometry | Findings fixed and rechecked | `feat: migrate tag editing and bulk actions to Solid`; `6eb9add` |
| [6. Lightbox and images](06-lightbox-and-images.md) | Complete | 1101 frontend, 206 Rust, both builds; desktop comparison, reduced motion and 12 matching layouts | Findings fixed and rechecked | `feat: migrate lightbox and image interactions to Solid`; `71943b0` |
| [7. Native video](07-native-video.md) | Complete | 1124 frontend, 206 Rust, both builds; 18 desktop cases per renderer and 30 native reopen cycles each | Findings fixed and rechecked | `feat: migrate native video lifecycle to Solid`; `cf59a58` |
| [8. Settings and maintenance](08-settings-and-maintenance.md) | Complete | 1205 frontend, 206 Rust, both isolated builds; 17 desktop cases and 3 reduced-motion cases per renderer | Findings fixed and rechecked | `feat: migrate settings and maintenance workflows to Solid`; `b401770` |
| [9. Parity and cutover](09-parity-and-cutover.md) | Complete | 622 final frontend, 206 Rust, final test:all with 39 desktop cases; 117 normal + 46 reduced-motion cases per renderer before cutover | Findings fixed and rechecked | `refactor: complete Solid migration and remove React` |
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
Local commit hash: `aa7c933`.

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

## Stage 3 evidence

Date: 4 October 2026. Starting commit: `aa7c933`.
Local commit subject: `feat: implement Solid library state and mutation coordination`.
Local commit hash: `64e7ac1`.

Implemented behavior:

- `createAppRuntime` owns one `createLibrary`, retained across Gallery/Settings navigation.
  The runtime constructs models without starting feature queries; stage 4 owns startup
  and search UI wiring. Models expose reactive getters and explicit commands.
- Query sessions retain 128-item pages, sparse global positions, the exact 12-page bound,
  viewport protection, per-offset deduplication, failed offsets, explicit retry, old results
  during refresh, superseded/stale handling, patch-counter rereads and disposal guards.
  Operational snapshots, generations and request registries update synchronously.
- One shared details cache retains at most 256 records, deduplicates reads, pins pending
  identities and checks metadata generations. Query changes invalidate details; identity
  reset, restore and clear invalidate details and canonical tags.
- Canonical tags retain per-asset ownership, tombstones, identity epochs, pins and the
  512-entry inactive bound. Maintenance admission is synchronous, drains dispatched writes,
  serializes operations and releases waiters on disposal. Rejected maintenance still resets
  the appropriate caches before releasing its barrier. Settings refresh/feedback follows
  in stage 8.
- Shared mutation commands publish backend-acknowledged tags, favorites, groups and deletion.
  Bulk admission is all-or-none. Stale identities cannot publish; partial results patch
  acknowledged IDs only. Current filters govern query impact. Pure query-impact predicates
  now live under library metadata and remain available through React's compatibility export.
- Vocabulary preserves pagination, stable equal snapshots, overlapping refresh draining,
  unchanged-save reuse and dirty retry. Thumbnail models reuse the pure store/constants,
  retain visible/additive priority and stream semantics, and own per-asset subscriptions.
  No React global thumbnail mirror is required by Solid.

Tests ported or added:

- Retained every React reference suite. Ported 62 cases from library-assets, library-browser,
  known-tags, tag-coordinator and thumbnail-queue suites. Solid queue assertions observe the
  real per-asset store; vocabulary setup hydrates through commands. Explicit Solid flushes
  replace React `act` only in test support. Pure service suites continue running once.
- Added 26 cases covering details eviction/reload and deduplication, stale identity/query
  reads, canonical mutation publication, same-turn exclusion, bulk admission/partial results,
  maintenance failure/disposal, latest-filter decisions, default page size, runtime lifetime,
  oversized range bounds, replaced request finalizers, sparse deletion positions, reactive
  asset-array stability, late page thumbnail demand and per-asset subscription cleanup.
- Solid now has 123 cases including the 35 shell/bootstrap cases from stages 1 and 2.

Verification, with logs under `artifacts/solid2/stage3/`:

| Command | Result | Evidence |
| --- | --- | --- |
| `bun run test` | 97 files, 822 passed | `frontend-final.log` |
| `bun run test --project solid` | 13 files, 123 passed | `solid-final.log` |
| `bun run test --project solid src/library/query` | 2 files, 24 passed after the final deleted-index guard | `query-final.log` |
| `bun run typecheck` | All three TypeScript contexts pass | `types.log` |
| `bun run test:frontends` | Both builds and renderer/entry separation pass; React assets restored in `dist` | `builds-final.log`, `react-modules.json`, `solid-modules.json` |
| `bun run test:backend` | 206 passed; 2 pre-existing diagnostic tests ignored | `backend.log` |
| `bun run lint` | Passed | `lint-final.log` |
| `bun run format:check` | Passed | `format.log` |
| `bun run locale:check` | All 11 languages valid | `locales.log` |
| `PATH="/tmp/solid2-tools:$PATH" bun run privacy:check` | Passed for history and tracked files; repeated after staging | `privacy.log`, `privacy-staged.log` |
| `git diff --check` | Passed | Working/staged diff review |

Self-review findings and fixes:

- A cleanup rename shadowed captured query/thumbnail generations and the coordinator epoch.
  Typechecking and race tests rejected it. Explicit current/captured names restore the
  guards; full frontend verification passed afterward. Initial failed logs are retained.
- Removing an item from a page's ID array would shift later global positions before refresh.
  Metadata patches now retain the snapshot IDs with an absent summary at the deleted slot.
  Cached range reads reject incomplete pages and index lookup rejects deleted summaries.
- A range larger than the cache can continuously reload evicted pages if every cache update
  repeats page demand. Range/query-epoch changes now drive page reads; page arrivals only
  update thumbnail demand. A 30-page integration case proves exactly 29 subsequent requests
  and 12 retained pages. Separate read memos prevent loading-only updates from repeating work.
- Initial per-tile subscription setup published an equal snapshot twice. Comparing ID, path
  and rendering state removes the duplicate update. Tests prove atomic completion, unrelated
  subscriber isolation, slot identity changes and retained-path release on disposal.
- The final caller review found that single-favorite saves await query refresh and deletion
  waits for both refreshes to settle. Shared commands now retain those completion rules;
  a deferred refresh regression verifies failure propagation after durable publication.
- Removed the temporary queue callback indirection and React setter/ref contracts. Reviewed
  actual callers, imports, cache bounds, synchronous admission, canonical publication,
  disposal and generation guards. Library production modules import no React or feature
  components; no production forced flushes were added.
- Reviewed staged source, test fixtures and documentation for scope and private content.
  Fixtures are synthetic; raw logs and compiled module lists remain ignored.

Remaining limits: no desktop run, packaging rebuild or cross-platform claim for this model
stage. Mocked IPC tests prove frontend coordination, not a live bridge or Solid gallery,
editor, lightbox or settings parity. Rust/IPC schemas, native profiles and backend behavior
are unchanged. Those feature integrations remain in their planned stages. No required
stage-3 check remains unavailable.

Next stage: [gallery, search and tag discovery](04-gallery-and-search.md).


## Stage 4 evidence

Date: 4 October 2026. Starting commit: `64e7ac1`.
Local commit subject: `feat: migrate gallery and search to Solid`.
Local commit hash: `9a87d96`.

Implemented behavior:

- The mounted Solid gallery consumes application-owned query, selection, vocabulary and
  thumbnail models. Startup runs once from `onSettled`. Gallery/Settings navigation retains
  search and selected IDs while remounting the gallery and resetting scroll.
- A narrow adapter uses the existing virtual-core 3.17.8, now an exact direct dependency.
  It waits for measured width, owns observers and cleanup, retains row geometry, overscan,
  scroll margin, resize anchoring and global indices. Slots use stable global-index keys;
  sparse arrivals replace placeholders and thumbnail subscriptions follow asset identity.
- Gallery tiles retain image/GIF presentation, group backplates, badges, selection previews,
  empty/error states and footer geometry. The shared Solid thumbnail image resets load/failure
  state per source and forwards load events, including cached-image handling.
- Pointer selection retains the 150 ms / 5 px threshold, capture, rectangle preview, Escape
  and query cancellation, click suppression and edge scrolling. The selection model resolves
  global ranges in 128-index windows, rejects late results and exposes explicit retry.
- Search commands compose drafts synchronously and apply the existing grammar, literal
  tokens, meta filters, media kinds and favorites. Unchanged valid submissions still query.
  Autocomplete retains keyboard/caret/IME behavior and one application-owned lazy worker.
- Tag discovery retains 100-tag pagination, local fallback, include/exclude click intent,
  focus and modal cleanup. Applying flushes pending clicks and keeps the selection on failure.
  Failed pagination without new fallback entries stops automatic retries until explicit input.

Tests ported or added:

- All 699 React/shared reference cases remain enabled. Solid now has 232 cases, including
  the 123 from previous stages. The full frontend run covers 111 files and 931 tests.
- Real virtual-core suites cover 10,000/26,000 slots, initially null refs, intermediate range
  bounds, row-measurement identity, margins, partial rows, resize anchors and disposal.
  Gallery integration uses the real query/cache composition, covers eviction beyond 12 pages,
  backward reload, sparse replacement, load retry, overlapping DOM identity and thumbnail
  subscriber isolation. Pointer tests port all 30 reference cases.
- Search tests cover explicit unchanged submission and same-batch commands; suggestion tests
  exercise request cancellation, stale replies, worker vocabulary reuse and disposal. Tag-list
  tests cover delayed click intent, fallback and pagination retry. Thumbnail tests retain all
  seven reference cases; footer tests cover all 11 locales.
- The application startup regression renders real composition with mocked IPC and proves
  initial results and explicit searches. Shell/bootstrap assertions now target the gallery;
  the obsolete shell-ready message was removed. The behavior inventory records new owners.

Verification, with local evidence under `artifacts/solid2/stage4/`:

| Command | Result | Evidence |
| --- | --- | --- |
| `bun run test` | 111 files, 931 passed | `frontend-final.log` |
| `bun run typecheck` | All three contexts pass | `types-final.log` |
| `bun run test:frontends` | Both builds and renderer/entry isolation pass; restores React assets in `dist` | `builds-final.log`, `react-modules.json`, `solid-modules.json` |
| `bun run test:backend` | 206 passed, 2 pre-existing diagnostics ignored | `backend.log` |
| `bun run lint` / `bun run format:check` / `bun run locale:check` | Passed; all 11 locales valid | `lint-final.log`, `format-final.log`, `locales-final.log` |
| `TMPDIR=/tmp BUN_INSTALL=/tmp/solid2-bun BUN_INSTALL_CACHE_DIR=/tmp/solid2-bun-cache bun install --frozen-lockfile --ignore-scripts` | Existing graph and new direct core declaration agree; no dependency version changed | `dependency-check.log` |
| `python3 artifacts/solid2/stage4/compare-geometry.py` | All 12 geometry samples exactly match; both cold/warm runs stay at or below 91 tiles | `geometry-parity.log` |
| `PATH="/tmp/solid2-tools:$PATH" bun run privacy:check` | Passed before and after staging | `privacy.log`, `privacy-staged.log` |
| `git diff --check` / `git diff --cached --check` | Passed | Working/staged diff review |

Desktop comparison used fresh temporary XDG profiles, private Xvfb/D-Bus sessions and
synthetic fixtures. The local wrapper from stage 2 retains cleanup guards. Each renderer
ran serially, with a fresh frontend/native build:

```sh
for renderer in solid react; do
  MEDIATAGGER_FRONTEND="$renderer" MEDIATAGGER_SOLID_SHELL=1 MEDIATAGGER_UI_REDESIGN=1 MEDIATAGGER_THUMBNAIL_FADE=1 MEDIATAGGER_GALLERY_SELECTION=1 MEDIATAGGER_GALLERY_BROWSING=1 bash artifacts/solid2/stage2/run-desktop.sh \
    --spec e2e/specs/solid-bootstrap.e2e.js \
    --spec e2e/specs/ui.redesign.e2e.js \
    --spec e2e/specs/thumbnail-fade.e2e.js \
    --spec e2e/specs/review-fixes.e2e.js \
    --spec e2e/specs/gallery-selection.e2e.js \
    --spec e2e/specs/gallery-browsing.e2e.js \
    --mochaOpts.grep 'Solid desktop bootstrap|Shell appearance|gallery footer|end-of-results footer|fades new images|applies literal tags|accidental quick drags|autoscrolls and resolves|gallery browsing parity'
done
```

Solid passed 22 cases; React passed 21 and skipped the Solid-only bootstrap case.
`desktop-solid-final.log` and `desktop-react-final.log` retain the results. The scenarios
cover persisted shell controls, both footers, thumbnail fading, literal tag include/exclude,
accidental quick drags and selection of all 2,051 items across unloaded ranges. The browsing
suite crosses 16 pages and returns to the beginning in cold/warm passes, verifies static GIF
thumbnails and gallery scroll reset, and records both languages/themes at three widths.

Repeated thumbnail tests with actual reduced motion enabled:

```sh
for renderer in solid react; do
  MEDIATAGGER_FRONTEND="$renderer" MEDIATAGGER_THUMBNAIL_FADE=1 MEDIATAGGER_REDUCED_MOTION=1 bash artifacts/solid2/stage4/run-reduced.sh --spec e2e/specs/thumbnail-fade.e2e.js
done
```

The ignored wrapper writes `gtk-enable-animations=false` into the temporary GTK settings,
sets `GSETTINGS_BACKEND=keyfile` and sets `org.gnome.desktop.interface enable-animations` to
false. The native scenario asserts the actual media preference. Both renderers passed two
cases each, recorded in `reduced-solid.log` and `reduced-react.log`.

A final Solid geometry capture waited for visible thumbnails to finish fading:

```sh
MEDIATAGGER_FRONTEND=solid MEDIATAGGER_GALLERY_BROWSING=1 bash artifacts/solid2/stage2/run-desktop.sh --spec e2e/specs/gallery-browsing.e2e.js --mochaOpts.grep 'preserves gallery and search geometry'
```

It passed one case, recorded in `geometry-solid-settled.log`. Inspected actual Solid
English/light 1440px and Polish/dark 320px pixels and the React English/light reference.
Header, search and tile rectangles match exactly across all 12 measured combinations, without
horizontal overflow. Fixture scan ordering differs between fresh databases, so screenshots
are visual evidence rather than an exact pixel-diff assertion. Cold/warm timings include
WebDriver overhead and remain diagnostics; DOM, cache and range bounds are the test gates.

Self-review findings and fixes:

- Starting library writes directly during Solid component construction left the native
  gallery empty. Startup now runs from `onSettled`; the application regression and native
  browser scenarios prove results arrive through the actual composed path.
- Index-position list reconciliation remounted overlapping thumbnails while scrolling.
  Global-index keys retain their DOM, and a real-core integration assertion checks identity.
- Thumbnail demand read reactive query data in an effect's apply phase. Those reads now live
  in computation; page arrivals update demand without repeating page reads or measurements.
- Image load/failure state must follow the source. A keyed image owner resets it and retains
  load callbacks. Tag chips also retain identity, preserving keyboard focus during selection.
- Automatic pagination could loop after a failed page exhausted local fallback. The new
  regression failed with three requests instead of two before the no-progress guard; it
  now passes and still allows an explicit retry.
- Tag-list initial focus now uses the modal's initial-focus marker. Input-frame cleanup,
  stale selection completion, worker lifetime and disposal were reviewed and tested.
- An early broad desktop run reached bulk group editing and lightbox assertions belonging
  to stages 5 and 6. Those scenarios remain unchanged; the final command selects complete
  stage-4 cases. The earlier failures remain in local logs and are not reported as passes.
- Initial sandbox D-Bus binding failed. Desktop runs then used approved local socket access.
  All final wrappers exited successfully and removed their temporary profiles.
- Reviewed production imports, sparse indices, geometry, ownership, cancellation, shared
  worker protocol, keyboard behavior, real module graphs, staged source and synthetic fixtures.
  No backend, IPC, native profile or React feature behavior changed. Raw logs, screenshots,
  media and databases remain ignored.

Remaining limits: the Solid selection inspector currently exposes count and Clear only.
Tag/favorite/group bulk editors follow in stage 5, tile activation/lightbox in stage 6,
native video in stage 7, and the Add first folder scan workflow and remaining settings in
stage 8. React remains the default and complete application. No cross-platform or packaging
parity claim is made; two existing Rust diagnostics remain ignored. No required stage-4
check remains unavailable.

Next stage: [shared tag editing and bulk actions](05-tagging-and-bulk.md).


## Stage 5 evidence

Date: 4 October 2026. Starting commit: `9a87d96`.
Local commit subject: `feat: migrate tag editing and bulk actions to Solid`.
Local commit hash: `6eb9add`.

Implemented behavior:

- `features/tagging/TagEditor.tsx` preserves wrapping chips, a controlled input, disabled and
  loading states, IME and suggestion behavior, viewport suggestions and field-only scrolling.
  Search and tag editors use the same application-owned suggestion worker.
- The runtime owns `createBulk` and loads popular tags once at startup. Selected summaries
  outlive gallery-page eviction and hydrate in 256-ID batches with four requests in flight.
  Failed or omitted summaries remain retryable. Committed metadata overlays older reads.
- Single-asset replacements require complete canonical tags. Multi-selection uses backend
  merges. Shared library mutations retain admission, canonical publication, query impact,
  vocabulary refresh, maintenance draining, partial results and identity invalidation.
  Command snapshots and operation locks are synchronous under Solid batching.
- Sidebar drafts, card errors and focus follow selection identity. A retired save cannot
  clear a new draft or focus its input. The 360px inspector and narrow placement use the
  existing CSS. Group initialization preserves a draft typed before metadata arrives.
- The group-order modal owns its draft and real virtual-core instance. Pointer capture,
  edge scrolling, keyboard ordering, announcements, thumbnail subscriptions and exact-order
  submission retain the reference behavior. Partial or failed saves preserve the draft;
  closing retains the final frame while releasing interaction ownership.

Tests ported or added:

- All 699 React/shared reference tests remain. Stage 5 adds 77 Solid tests across the editor,
  sidebar/modal and bulk model suites. Stage-4 selection and stage-3 metadata suites continue
  covering shared coordination. Pure merge/order helpers and their tests remain shared.
- Deferred reads and writes cover contention, duplicate commands, missing and offscreen IDs,
  partial merges, failed refreshes, stale selection/identity results, disposal and late summary
  hydration. Group tests cover exact orders, single-asset order preservation, failure/retry,
  cancellation, closing frames and a thousand-item DOM/request bound.
- Desktop tag-editor persistence now runs independently for bulk, alongside the original
  combined lightbox/bulk case. It retains canonical tag reads after reload and both unchanged
  and changed vocabulary IPC assertions. The short-gallery gesture starts in the measured
  gallery section, with its original selection assertions intact.

Verification, with logs under `artifacts/solid2/stage5/`:

| Command | Result | Evidence |
| --- | --- | --- |
| `bun run test` | 114 files, 1008 passed | `frontend-final.log` |
| `bun run typecheck` | All three contexts pass | `types-final.log` |
| `bun run test:frontends` | Both builds and renderer/entry isolation pass; React assets restored in `dist` | `builds-final.log`, `react-modules.json`, `solid-modules.json` |
| `bun run test:backend` | 206 passed; 2 existing diagnostics ignored | `backend.log` |
| `bun run lint` | Passed | `lint-final.log` |
| `bun run format:check` | Passed | `format.log` |
| `bun run locale:check` | All 11 languages valid | `locales.log` |
| `PATH="/tmp/solid2-tools:$PATH" bun run privacy:check` | Passed before and after staging | `privacy.log`, `privacy-staged.log` |
| `git diff --check` / `git diff --cached --check` | Passed | Working/staged diff review |
| `python3 artifacts/solid2/stage5/compare-geometry.py` | All four editor geometry comparisons match exactly | `geometry-parity.log` |

Desktop comparison uses the existing private Xvfb/D-Bus wrapper, fresh temporary profiles,
synthetic files and serial renderer builds:

```sh
for renderer in solid react; do
  MEDIATAGGER_FRONTEND="$renderer" MEDIATAGGER_TAG_EDITOR=1 MEDIATAGGER_GALLERY_SELECTION=1 MEDIATAGGER_GROUP_ORDER=1 bash artifacts/solid2/stage2/run-desktop.sh \
    --spec e2e/specs/tag-editor.e2e.js \
    --spec e2e/specs/gallery-selection.e2e.js \
    --spec e2e/specs/group-order.e2e.js \
    --mochaOpts.grep 'keeps bulk|persists bulk|ignores accidental|autoscrolls and resolves|preserves selection on empty|drags, cancels'
done
```

Both renderers passed seven cases, recorded in `desktop-solid-final.log` and
`desktop-react-final.log`. These exercised both themes, 1100px and 600px tag fields, field-only scrolling and focus, persisted tags, vocabulary IPC counts, selection of
2,051 IDs, short-gallery empty space, pointer/keyboard group ordering, cancellation, edge
scrolling and persisted order. Screenshots and geometry JSON remain under `artifacts/tag-editor/`
and `artifacts/group-order/`. Inspected Solid light/1100px and dark/600px screenshots and the
React light/1100px reference. Field/input rectangles, client sizes and scroll sizes match
exactly across both themes and both widths in the final captured geometry JSON.

Self-review findings and fixes:

- Solid requires returning cleanup from `onSettled`. The first editor test run rejected an
  `onCleanup` registration there; the observer now returns its cleanup.
- A props refresh with unchanged selected IDs cancelled pending focus. Sidebar reset now
  compares the actual selected-set identity. Disabled-input keyboard submission is guarded,
  and clicking an enabled field reopens current suggestions. Ported regression tests pass.
- A late offscreen summary could overwrite a durable favorite change. Selected metadata
  retains committed patches for the selected IDs and overlays them on older reads. Unrelated
  deletion notifications do not create errors or retained tombstones in the selection model.
- Modal initialization followed metadata updates even while it remained open, discarding the
  draft during acknowledgement. A memo of logical open state now owns initialization and
  listener lifetime. The added deferred-save regression failed before the fix and passes now.
- Disabling virtual-core on close removed the visible rows before the exit fade. A modal
  snapshot retains rows, total size and display metadata until the presence owner removes DOM.
- Removed copied ref-holder objects from the Solid modal, replaced mapped JSX with owned
  `For` lists, and replaced quadratic selected-ID lookup with a memoized set. Reviewed
  synchronous admission, selection identity, canonical ownership, listeners, pointer capture,
  frames, observers, virtualizer disposal, thumbnails and renderer dependency direction.
  Canonical tags published by another owner also enable an editor whose older details read
  is still pending; a deferred regression verifies that the old read cannot replace them.
- The initial short-gallery desktop case failed in both renderers because viewport-based
  coordinates landed on the footer. It now measures the gallery section and asserts that
  the starting point is below the tiles. A reload assertion also needed to await detail
  hydration; it now waits for the same persisted tag text instead of reading a loading frame.
  Earlier failure logs remain local and are not counted as passes.
- The first sandbox desktop launch could not bind D-Bus. Approved local socket access allowed
  the isolated wrapper to run. Temporary profiles were removed by its existing cleanup guard.

Remaining limits: Solid lightbox activation and image behavior remain stage 6, native video
stage 7, and complete settings/maintenance stage 8. The shared editor is tested with its
lightbox-ready contract, but native lightbox integration is not claimed. This stage does not
establish cross-platform or packaging parity. Two pre-existing Rust diagnostic tests remain
ignored. No required stage-5 check remains unavailable. Reviewed staged source, synthetic
fixtures and documentation for scope and private content. Raw logs, media, screenshots and
databases remain ignored; no backend, IPC or native profile contract changed.

Next stage: [lightbox selection and images](06-lightbox-and-images.md).

## Stage 6 evidence

Date: 4 October 2026. Starting commit: `6eb9add`.
Local commit subject: `feat: migrate lightbox and image interactions to Solid`.
Local commit hash: `71943b0`.

Implemented behavior:

- `features/lightbox/createLightbox.ts` owns selection and editors under the application.
  Global navigation accumulates rapid commands, wraps across pages, rejects retired lookups
  and resolves the selected ID after query reorder. The query exposes a synchronous navigation
  snapshot, independently of batched reactive publication.
- Shared complete details retain the existing 256-entry bound and generation validation.
  Current reads wait for metadata writes and refetch stale records. Adjacent prefetch, query
  invalidation, missing/failed positions, independent details/save retry and reused IDs are
  covered. Summaries never become an authoritative tag base.
- Tag replacements serialize and coalesce per ID across selection changes and close/reopen.
  Busy or failed saves retain intent; Retry rebases it onto an external canonical winner.
  Favorite, group and deletion actions use the shared backend-first mutation coordinator.
  Dirty group fields survive hydration and unrelated metadata publications.
- The lazy image/GIF modal preserves the existing CSS, shared tag editor, sidebar/drawer,
  action rail, info and clipboard controls, fullscreen, idle behavior, delete confirmation,
  shortcuts and focus priority. Image/GIF DOM and zoom survive metadata-only updates.
  Pointer capture, frames, observers and listeners follow the mounted presentation owner.
- Logical close releases modality immediately. The exit frame retains all displayed model
  values and sidebar choice. Asset navigation retains the shell; close/reopen creates a new
  presentation owner. The app captures the gallery focus target before lazy loading.
- Native video has a presentation boundary but remains stage 7. React stays the default.
  Command names, backend behavior, database schema and app-data profiles are unchanged.

Tests ported or added:

- 93 Solid cases across five suites observe selection/mutations, modal interactions, fitted
  image rendering, group input validation and presentation lifetime. The original 699
  React/shared cases remain enabled. Library tests continue proving cache bounds and shared
  coordination; pure fit/pan, tag and group helpers remain shared.
- Native image/GIF animation cases now run independently for each media kind with their
  original assertions. The video case remains enabled for stage 7. The image comparison
  suite records both themes and English/Polish at 320, 600 and 1000px, and checks suggestion
  Escape and gallery focus restoration. All fixtures are synthetic and disposable.

Self-review findings and fixes:

- Reading reactive query getters immediately after refresh still observed the old session.
  A regression failed before the new synchronous navigation snapshot and passes after it.
  No forced frontend flush was added to production code.
- An image-control effect reset zoom for every metadata object. A memo now compares only
  ID and intrinsic dimensions. Both image and GIF reference regressions failed before the
  fix and pass while retaining the actual image node and original-resolution sizing.
- Exit state now snapshots all displayed model values, including failed save feedback.
  The sidebar keeps its explicit choice until its presentation owner exits. The regression
  checks collapsed state, unsaved chips and Retry during the fade.
- Tag admission reuses `isValidTag`, including control-character and duplicate rejection.
  Focus restoration covers the lazy loading dialog and loaded media view. Clipboard
  acknowledgements cannot mark another selection as copied.
- Native sidebar measurements initially caught fractional button bounds during the drawer
  animation. The test waits for the actual CSS `translate` property and running animations;
  a `transform`-only wait was insufficient. Exact dimension assertions remain unchanged.
- The new suggestion fixture initially assigned every known tag to the selected image, so
  its expected suggestion was correctly absent. A second synthetic image supplies a known,
  unassigned tag. The geometry fixture explicitly selects one filename across renderers.
- Initial component test failures came from missing `matchMedia`, exact translated labels,
  combobox roles and highlighted accessible names. Corrected the test setup and queries;
  the final suite has no skipped cases or suppressed Solid diagnostics.
- Reviewed callers, canonical publication, query/session changes, same-turn admission,
  stale async completion, owner cleanup, GIF identity, modal presence and dependency direction.
  The lightbox docs incorrectly claimed all media waited for complete details. The reference
  accepts verified GIF/video preview paths while full tags load; owning docs now say so.

Verification, with local evidence under `artifacts/solid2/stage6/`:

| Command | Result | Evidence |
| --- | --- | --- |
| `bun run test` | 119 files, 1101 passed | `frontend-final.log` |
| `bun run typecheck` | All three contexts pass | `types-final.log` |
| `bun run test:backend` | 206 passed; 2 existing diagnostics ignored | `backend-final.log` |
| `bun run lint` / `bun run format:check` | Passed | `lint-final.log`, `format-final.log` |
| `bun run locale:check` | All 11 languages valid | `locales-final.log` |
| `bun run test:frontends` | Both builds and renderer/entry isolation pass; React assets restored in `dist` | `builds-final.log`, `react-modules.json`, `solid-modules.json` |
| `python3 artifacts/solid2/stage6/compare-geometry.py` | All 12 layout measurements match exactly | `geometry-parity.log` |
| `PATH="/tmp/solid2-tools:$PATH" bun run privacy:check` | Passed before and after staging | `privacy-final.log`, `privacy-staged.log` |
| `git diff --check` / `git diff --cached --check` | Passed | Working/staged diff review |

Desktop checks used the existing private Xvfb/D-Bus wrappers, temporary XDG profiles,
synthetic files, guarded cleanup and serial renderer builds. The broad comparison command was:

```sh
for renderer in solid react; do
  MEDIATAGGER_FRONTEND="$renderer" MEDIATAGGER_TAG_EDITOR=1 MEDIATAGGER_ANIMATIONS=1 MEDIATAGGER_LIGHTBOX_ACTIVITY=1 MEDIATAGGER_LIGHTBOX_IMAGES=1 bash artifacts/solid2/stage2/run-desktop.sh \
    --spec e2e/specs/tag-editor.e2e.js \
    --spec e2e/specs/review-fixes.e2e.js \
    --spec e2e/specs/app.workflows.e2e.js \
    --spec e2e/specs/animations.e2e.js \
    --spec e2e/specs/lightbox-activity.e2e.js \
    --spec e2e/specs/lightbox-images.e2e.js \
    --mochaOpts.grep 'keeps lightbox|persists lightbox|navigates in the new group|keeps lightbox panels|uses a drawer|supports lightbox tagging|modal media lifecycle (image|gif)|keeps the bottom box|fades controls and restores focus without resizing (image|gif)|image lightbox parity'
done
```

React passed all 21 cases in `desktop-react-final.log`. The Solid broad run passed 19 and
failed two test-setup cases, recorded in `desktop-solid-review.log`. After the drawer wait
and suggestion-fixture fixes described above, this final Solid recheck passed all eight cases:

```sh
MEDIATAGGER_FRONTEND=solid MEDIATAGGER_TAG_EDITOR=1 MEDIATAGGER_ANIMATIONS=1 MEDIATAGGER_LIGHTBOX_ACTIVITY=1 MEDIATAGGER_LIGHTBOX_IMAGES=1 bash artifacts/solid2/stage2/run-desktop.sh \
  --spec e2e/specs/tag-editor.e2e.js \
  --spec e2e/specs/animations.e2e.js \
  --spec e2e/specs/lightbox-activity.e2e.js \
  --spec e2e/specs/lightbox-images.e2e.js \
  --mochaOpts.grep 'keeps lightbox|persists lightbox|modal media lifecycle (image|gif)|keeps the bottom box.*600px|image lightbox parity'
```

`desktop-solid-final.log` records the recheck. Together the runs cover all 21 distinct cases
for Solid, including persisted tags/groups and deletion, navigation after a group reorder
beyond page one, responsive panels, focus, fullscreen, image/GIF animation and idle controls.
Earlier failures remain in local logs and are not counted as successful runs.

Actual reduced-motion checks ran separately:

```sh
for renderer in solid react; do
  MEDIATAGGER_FRONTEND="$renderer" MEDIATAGGER_ANIMATIONS=1 MEDIATAGGER_LIGHTBOX_ACTIVITY=1 MEDIATAGGER_REDUCED_MOTION=1 bash artifacts/solid2/stage4/run-reduced.sh \
    --spec e2e/specs/animations.e2e.js \
    --spec e2e/specs/lightbox-activity.e2e.js \
    --mochaOpts.grep 'modal media lifecycle (image|gif)|fades controls and restores focus without resizing (image, light, 600px|gif, dark, 1000px)'
done
```

Each renderer passed four cases, recorded in `reduced-solid.log` and `reduced-react.log`.
The wrapper disables GTK animations in the temporary profile; the tests assert the actual
media preference. Image/GIF lifecycle, fullscreen, idle controls and focus passed.

The first geometry comparison found one cached drawer scroll extent in React after its
translate animation. The test now also waits for the layout's scroll extent to settle.
It keeps the exact rectangle and overflow assertions. Both renderers then passed both
image comparison cases again:

```sh
for renderer in solid react; do
  MEDIATAGGER_FRONTEND="$renderer" MEDIATAGGER_LIGHTBOX_IMAGES=1 bash artifacts/solid2/stage2/run-desktop.sh --spec e2e/specs/lightbox-images.e2e.js
done
```

`geometry-solid-final.log` and `geometry-react-final.log` record these runs. The comparison
script reads all 12 samples from `images-solid/geometry.json` and `images-react/geometry.json`.
Every rectangle, client size, scroll width and document-overflow flag agrees exactly across
English/Polish, light/dark and 320/600/1000px. Inspected actual Solid desktop and narrow Polish
screenshots and the React desktop reference. Timing remains diagnostic on this Linux host.

Reviewed staged source, test fixtures and documentation for scope and private content.
Raw logs, screenshots, media and disposable databases remain ignored. No backend, wire
payload, dependency or app-data-profile change is included.

Remaining limits: Solid native video playback is stage 7; full settings/maintenance is stage 8.
React remains the default complete application. This stage does not establish cross-platform
or packaging parity. Two pre-existing Rust diagnostics remain ignored. No required stage-6
check remains unavailable.

Next stage: [native video integration](07-native-video.md).


## Stage 7 evidence

Date: 4 October 2026. Starting commit: `71943b0`.
Local commit subject: `feat: migrate native video lifecycle to Solid`.
Local commit hash: `cf59a58`.

Implemented behavior:

- `features/lightbox/video/createNativeVideoSession.ts` owns begin/open/cancel tokens,
  returned-session event filtering, bounded early events, serialized controls, native bounds
  acknowledgments and cleanup. Labels and callbacks update without replacing playback.
- The native player uses the existing video platform wrappers and shared bounds helper.
  Asset ID/kind/path changes and Retry create new media owners. Logical close disposes the
  player immediately, including pending opens, while the DOM shell finishes its exit.
- The lightbox reserves windowed top controls, waits for native clearance before revealing
  the sidebar, hides narrow video through drawer exit and restores focus afterward. Fullscreen
  retains the player/session while hiding DOM panels and removing the top reservation.
- Video-target keys seek or toggle pause without navigating the library. Native pointer events
  reset the activity timer. Translated labels update the active GTK controls. Playback rate,
  volume and mute remain backend-owned, with no change to commands or backend behavior.
- Leaving native fullscreen for an image/GIF restores the shell's browser fullscreen state.
  A corrupt generated original can recover through the existing Retry UI.

Self-review findings and fixes:

- The first dependency tuple invalidated the session when only labels or callbacks changed.
  The ported label-update test failed. A memo now preserves asset/generation/container identity
  across these updates, and the complete session suite passes.
- Native desktop playback exposed Solid's boolean data-attribute serialization. Explicit
  string values preserve `data-native-paused`, seeking, buffering and muted snapshots. A
  rendered-player regression observes both false and true values rather than only signal state.
- Review found stale fullscreen layout when navigating from native video to an image.
  `fullscreen-transition-regression.log` fails on the actual shell's `rounded-none` class.
  Media-kind changes now resynchronize browser fullscreen state; the regression passes in
  `fullscreen-transition-fixed.log`. Its initial fixture lacked `toMediaSrc`; that fixture
  failure is recorded separately in `fullscreen-transition-before.log`.
- The native failure test initially queried every Retry button, including query-position
  retry. It now finds the button inside the media error. The model fixture also initializes
  a real query model so unrelated navigation failure does not contaminate the test.
- React comparison exposed a test race in the windowed-toggle scenario. Its first size sample
  preceded the sidebar's retained-width release, and its supposed idle wait read a child's
  opacity instead of the fading parent. The test now waits for the one-column layout and
  finished drawer animation, then observes the actual parent fade. The exact size equality
  and native-clearance assertions remain. `native-react.log` preserves the original failure.
  A later Solid recheck caught WebDriver clicking the still-entering sidebar and retrying
  after it moved. The test now also waits for acknowledged visibility, completed entrance
  and button hit testing before its first collapse click.
- React native fullscreen failed when the XTest helper batched movement and button press
  over idle DOM controls. Subsequent cases inherited its paused state. The helper now moves
  the real pointer, waits for the button to be reachable through DOM hit testing, then clicks.
  It does not change playback state or bypass native input. The affected scenarios are rerun
  after this change; the original failures remain in `native-react.log`.
- Reviewed current and retired callbacks, queued commands, late open/label/bounds completion,
  owner disposal, retained exit values, narrow focus timing and metadata-only updates.
  The IPC guide incorrectly said every medium waits for details. It now describes the verified
  GIF/video preview paths already used by both implementations.

Verification, with local evidence under `artifacts/solid2/stage7/`:

| Command | Result | Evidence |
| --- | --- | --- |
| `bun run test` | 121 files, 1124 passed | `frontend-final.log` |
| `bun run typecheck` | All three contexts pass | `types-final.log` |
| `cargo test --manifest-path src-tauri/Cargo.toml video_ --lib` | 17 passed, including real libmpv worker tests with null output | `rust-video.log` |
| `bun run test:backend` | 206 passed; 2 existing diagnostics ignored | `backend-final.log` |
| `bun run lint` / `bun run format:check` | Passed | `lint-final.log`, `format-final.log` |
| `bun run locale:check` | All 11 languages valid | `locales-final.log` |
| `PATH="/tmp/solid2-tools:$PATH" bun run privacy:check` | Passed | `privacy-final.log` |

Solid's first desktop run passed seven of eight cases. The snapshot boolean assertion failed
in `keeps native playback state`; `desktop-solid-review.log` preserves that failure. After
the explicit string-attribute fix, all three video workflow cases passed, and six native
speed/recovery cases passed in `native-solid.log`. These commands used generated media,
disposable XDG profiles, authenticated private Xvfb and the existing wrapper:

```sh
MEDIATAGGER_FRONTEND=solid MEDIATAGGER_NATIVE_VIDEO=1 MEDIATAGGER_ANIMATIONS=1 MEDIATAGGER_LIGHTBOX_ACTIVITY=1 bash artifacts/solid2/stage2/run-desktop.sh \
  --spec e2e/specs/app.workflows.e2e.js \
  --spec e2e/specs/animations.e2e.js \
  --spec e2e/specs/lightbox-activity.e2e.js \
  --mochaOpts.grep 'windowed video lightbox|keeps native playback|loads generated GIF|modal media lifecycle video|fades controls and restores focus without resizing video'

MEDIATAGGER_FRONTEND=solid MEDIATAGGER_NATIVE_VIDEO=1 bash artifacts/solid2/stage2/run-desktop.sh \
  --spec e2e/specs/app.workflows.e2e.js \
  --spec e2e/specs/video-speed.e2e.js \
  --mochaOpts.grep 'windowed video lightbox|keeps native playback|loads generated GIF|native video speed and reopening'
```

The first invocation inside the sandbox could not bind its private D-Bus socket. Elevated
execution of the same isolated wrapper succeeded; no application code or isolation guard
was changed to accommodate the environment.

The native suite used actual XTest clicks on GTK controls and screenshot pixel comparisons,
not only DOM snapshots. Solid's 30 cycles passed at all six rates, with alternating same-file
reopens and two-file replacement. Every capture pair contained colored, changing decoded
pixels. The minimum color fraction was 0.6667 and minimum mean frame difference was 9.4781.
Corrupt-original Retry recovery also passed its pixel check. Raw evidence remains in
`artifacts/video-speed/2026-10-04T13-14-23-454Z/`, including `cycles.json` and `recovery.json`.
Inspected the actual desktop and narrow-window screenshots, including GTK controls and
clearance above the picture. Backend startup identified `GdkX11Display`.

The fullscreen-video to image fix followed this native run. Its focused regression passes.
The broad React comparison ran this command and passed 11 of 15 cases. Its four failures
were the geometry timing and native click issues described above; final rechecks follow.

```sh
MEDIATAGGER_FRONTEND=react MEDIATAGGER_NATIVE_VIDEO=1 MEDIATAGGER_ANIMATIONS=1 MEDIATAGGER_LIGHTBOX_ACTIVITY=1 bash artifacts/solid2/stage2/run-desktop.sh \
  --spec e2e/specs/app.workflows.e2e.js \
  --spec e2e/specs/video-speed.e2e.js \
  --spec e2e/specs/animations.e2e.js \
  --spec e2e/specs/lightbox-activity.e2e.js \
  --mochaOpts.grep 'windowed video lightbox|keeps native playback|loads generated GIF|native video speed and reopening|modal media lifecycle video|fades controls and restores focus without resizing video'
```

React passed all 30 reopen cycles and pixel comparisons, corrupt-original recovery,
fullscreen-to-GIF navigation, animation lifecycle and four activity scenarios in that run.
Its raw pixel evidence is under `artifacts/video-speed/2026-10-04T13-21-13-525Z/`. The minimum
color fraction was 0.6962 and minimum mean frame difference was 10.9338. These measurements
are diagnostics on the same host; the existing pixel assertions remain the acceptance gate.

Final native rechecks:

- `native-react-final.log`: all five selected cases passed after the pointer and geometry
  corrections. This includes the four previously failing cases and mixed-media navigation.
- `native-solid-final.log`: fullscreen across both themes, both widths and all three focus
  locations passed, as did mixed-media navigation on the final production source. The
  entering-sidebar click failed in the geometry case, as described above.
- `geometry-solid-final.log` and `geometry-react-final.log`: one case each passed after the
  entrance wait. Both assert exact video-size preservation through actual inactivity and
  native clearance above and beside the picture.

The final native rechecks used:

```sh
MEDIATAGGER_FRONTEND=react MEDIATAGGER_NATIVE_VIDEO=1 bash artifacts/solid2/stage2/run-desktop.sh \
  --spec e2e/specs/app.workflows.e2e.js --spec e2e/specs/video-speed.e2e.js \
  --mochaOpts.grep 'windowed video lightbox|exits bottom-button fullscreen|dismisses outside clicks|selects every rate|restores windowed GIF geometry'
MEDIATAGGER_FRONTEND=solid MEDIATAGGER_NATIVE_VIDEO=1 bash artifacts/solid2/stage2/run-desktop.sh \
  --spec e2e/specs/app.workflows.e2e.js --spec e2e/specs/video-speed.e2e.js \
  --mochaOpts.grep 'windowed video lightbox|exits bottom-button fullscreen|restores windowed GIF geometry'
for renderer in solid react; do
  MEDIATAGGER_FRONTEND="$renderer" bash artifacts/solid2/stage2/run-desktop.sh \
    --spec e2e/specs/app.workflows.e2e.js --mochaOpts.grep 'windowed video lightbox'
done
```

Together the successful runs cover all 15 distinct desktop scenarios for each renderer.
Earlier failed invocations remain recorded and are not counted as successful runs.
Actual reduced-motion checks then passed all three cases per renderer, using the temporary
GTK preference and asserting `prefers-reduced-motion` inside the WebView:

```sh
for renderer in solid react; do
  MEDIATAGGER_FRONTEND="$renderer" MEDIATAGGER_ANIMATIONS=1 MEDIATAGGER_LIGHTBOX_ACTIVITY=1 MEDIATAGGER_REDUCED_MOTION=1 bash artifacts/solid2/stage4/run-reduced.sh \
    --spec e2e/specs/animations.e2e.js --spec e2e/specs/lightbox-activity.e2e.js \
    --mochaOpts.grep 'modal media lifecycle video|fades controls and restores focus without resizing (video, light, 600px|video, dark, 1000px)'
done
bun run test:frontends
```

`reduced-solid.log` and `reduced-react.log` record the three-case runs. The normal and reduced
runs together cover 18 distinct desktop cases per renderer. `builds-final.log` proves both
entries compile with the correct renderer and no other renderer bundled. The module lists
are copied to `solid-modules.json` and `react-modules.json`; React assets are restored in
`dist`. Inspected Solid desktop/narrow screenshots, the React reference, and the actual
windowed GIF after native fullscreen navigation.

Reviewed staged source, fixtures and documentation for scope and private content. Final
`git diff --check` and `git diff --cached --check` pass; the staged privacy check is recorded
in `privacy-staged.log`. Raw logs, screenshots, synthetic media and temporary databases stay
ignored. No backend, wire payload, dependency or profile change is included.

Remaining limits: native desktop verification used Linux X11 with llvmpipe basic rendering.
The generated MP4s contain no audio track, so audible output, hardware GPU rendering, Wayland,
other operating systems and release packaging are not established by this stage. The two
pre-existing Rust diagnostics remain ignored. No required stage-7 check remains unavailable.

Next stage: [settings and maintenance workflows](08-settings-and-maintenance.md).

## Stage 8 evidence

Date: 4 October 2026. Starting commit: `cf59a58`.
Local commit subject: `feat: migrate settings and maintenance workflows to Solid`.
Local commit hash: `b401770`.

Implemented behavior:

- `createAppRuntime` owns `features/settings/createSettings.ts` and its scan, import/export,
  duplicate and danger models. It hydrates roots and dispatches startup scanning once.
  Operations, messages, progress and confirmations survive gallery/settings remounts.
  The first-folder route highlights scanning; Back clears the highlight.
- The shared runner acquires its lock synchronously, subscribes before dispatch, filters
  progress phases, translates backend errors and releases loading/locks in `finally`.
  Owner disposal removes listeners, including late registration. Thumbnail stop requests
  have their own synchronous guard and leave the original operation's lock intact.
- All seven settings sections, navigation and confirmations use the existing markup,
  styling, translations and platform commands. About bundles the exact legal documents
  and publisher metadata, and keeps the native opener's failure/copy behavior.
- CSV, restore and clear drain the actual metadata coordinator before backend dispatch,
  hold maintenance through identity/cache invalidation, and refresh after release.
  Restore retains vocabulary until refresh; clear empties it immediately. Root removal,
  scans, thumbnail rendering and duplicate batches keep their separate refresh paths.
- The duplicate resolver stages one change per ID, validates through the shared helper,
  virtualizes groups and subscribes to thumbnail updates per asset. Mixed batches require
  the nested delete confirmation and send one revision/path/version-checked backend batch.
  Confirmation dialogs retain their visible path/counts/disabled state during exit.
- Shared settings helpers now import the framework-independent i18n core. React remains
  runnable and default; Solid's bundle contains no React or React i18n adapter.

Tests written or ported:

- Settings action, confirmation, startup scan, section/status, About, panel and duplicate
  dialog suites retain the reference behavior. Resolver tests observe real drafts and
  staged commands instead of React setter spies. Pure progress and validation suites
  remain shared. All existing React tests remain enabled.
- New runner tests cover same-turn exclusion, failed/late registration, disposal, stale
  progress and backend error formats. Maintenance tests use the real tag coordinator to
  prove draining, denied writes, invalidation ordering and recovery after uncertain errors.
- Measured virtual-list regression, retained confirmation content, first-folder navigation,
  root mappings, separate stop ownership and restore/clear reset differences have direct
  regression assertions. The behavior inventory names the owning suites.

Self-review findings and fixes:

- A rescan removing duplicate groups exposed old virtual indices against the new list.
  The live desktop fell into the lazy error boundary while retaining the resolver portal.
  The original zero-size DOM test did not exercise virtual rows. After supplying actual
  viewport dimensions, `duplicate-rescan-measured.log` reproduces the failure. Each rendered
  row now carries its group from the same snapshot; `duplicate-rescan-fixed.log` passes.
  Desktop batches subsequently completed, and filesystem plus reloaded rows matched.
- Screenshot inspection caught cleared confirmation counts changing to zero during exit.
  Settings confirmations now use `createModalSnapshot`. The new presence tests verify the
  old counts and root path while the dialog is logically closed but still animating.
- Build isolation initially found four `react-i18next` modules imported by shared settings
  helpers. Their three imports now point to `i18n/core`; both isolated builds pass. No
  duplicate translations or renderer bridge was introduced.
- About's WebDriver scroll left the address below a nested scroll viewport. Waiting alone
  still failed, recorded in `desktop-diagnostics.log` and the ignored geometry JSON.
  The test now calls the element's native `scrollIntoView` and waits for hit testing before
  its actual XTest focus click. Selection and native capability-rejection assertions remain.
- The new duplicate desktop test initially inspected `file_name` on legacy `list_assets`
  rows, which expose `path`. It now compares the full persisted path with the verified
  renamed file. The backend and UI assertions were retained. Narrow navigation centers its
  actual link before clicking to avoid the sticky header.
- Ported tests now create JSX under the Solid owner and install spies before DOM event
  registration. Existing shell mocks provide real-shaped roots/startup responses, and the
  direct runtime test provides its language owner. These fixture failures were not treated
  as production successes.
- Reviewed synchronous confirmations/cancellation, scan snapshot identity, partial failure,
  maintenance release, listener cleanup, thumbnail ownership, modal exits and lazy imports.
  The settings guide now corrects two stale reference descriptions: duplicate application
  opts out of global loading, and resetting the thumbnail queue clears its shared store.

Verification evidence stays under ignored `artifacts/solid2/stage8/`:

| Command | Result | Evidence |
| --- | --- | --- |
| `bun run test` | 137 files, 1205 passed | `frontend-final.log` |
| `bun run typecheck` | React, Solid and configuration contexts pass | `types-final.log` |
| `bun run test:backend` | 206 passed; 2 existing diagnostics ignored | `backend-final.log` |
| `bun run lint` / `bun run format:check` | Passed | `lint-final.log`, `format-final.log` |
| `bun run locale:check` | All 11 locales valid | `locales-final.log` |
| `bun run test:frontends` | Both builds and renderer/entry isolation pass; React restored in `dist` | `builds-final.log`, `solid-modules.json`, `react-modules.json` |

The full desktop comparison runs serially with private Xvfb/D-Bus, fresh temporary XDG
profiles, the E2E title/profile guards and synthetic media:

```sh
for renderer in solid react; do
  MEDIATAGGER_FRONTEND="$renderer" MEDIATAGGER_SETTINGS_MAINTENANCE=1 MEDIATAGGER_SETTINGS_LANGUAGE=1 MEDIATAGGER_ABOUT=1 bash artifacts/solid2/stage2/run-desktop.sh \
    --spec e2e/specs/app.workflows.e2e.js \
    --spec e2e/specs/startup-scan.e2e.js \
    --spec e2e/specs/settings-language.e2e.js \
    --spec e2e/specs/settings-maintenance.e2e.js \
    --spec e2e/specs/about.e2e.js \
    --mochaOpts.grep 'removes scan root|clears library data|roundtrips tags|restores library state|per-folder startup scans|both cards|settings maintenance ownership|About and repository links'
done
```

`desktop-solid-final.log` and `desktop-react-final.log` each pass 17 cases. After the
confirmation snapshot fix, the same three maintenance cases pass in
`maintenance-solid-final.log` and `maintenance-react-final.log`. After correcting the shared i18n imports, the full 17-case comparison passed again in
`desktop-solid-isolated.log` and `desktop-react-isolated.log` on the final production source. Earlier failed diagnostic runs remain local
and are not counted as passes.

Actual reduced-motion runs also pass three cases per renderer, recorded in
`reduced-solid.log` and `reduced-react.log`. The wrapper disables GTK animations in the
temporary profile; the suite asserts the WebView's actual media preference:

```sh
for renderer in solid react; do
  MEDIATAGGER_FRONTEND="$renderer" MEDIATAGGER_SETTINGS_MAINTENANCE=1 MEDIATAGGER_REDUCED_MOTION=1 bash artifacts/solid2/stage4/run-reduced.sh \
    --spec e2e/specs/settings-maintenance.e2e.js
done
```

The desktop coverage includes startup preference persistence and fresh-process semantics,
root removal/clear confirmations, CSV/bundle roundtrips, settings unmount during a real scan,
preference changes while locked, mixed duplicate cancellation/application, and files plus
rows after reload. Language layout covers English/Polish at 1440/600/320px; About checks
both themes/languages, exact offline text and native opener rejection with address selection.
Inspected actual Solid and React duplicate-dialog screenshots, the resolved Solid dialog,
narrow Polish settings and the selected About address.

Remaining limits: native chooser interactions are mocked in frontend tests; desktop CSV
and bundle roundtrips use real commands directly. Desktop verification uses Linux X11;
other platforms, release packaging and successful external-browser launch are not established
by this stage. The two pre-existing Rust diagnostics remain ignored. No required stage-8
check remains unavailable. Raw logs, screenshots, synthetic media and temporary databases
remain ignored.

Reviewed all staged source, fixtures and documentation for scope and private content.
`git diff --check` and `git diff --cached --check` pass. The staged privacy check is recorded
in `privacy-staged.log`; no media, databases, logs or screenshots are included in the commit.
No backend, wire payload, dependency version or profile change is included.

Next stage: [parity, cutover, and React removal](09-parity-and-cutover.md).

## Stage 9 evidence

Date: 4 October 2026. Starting commit: `b401770`.
Decision trail: [STAGE9-DECISIONS.tsv](STAGE9-DECISIONS.tsv).
All raw logs, timing samples and screenshots below are ignored under
`artifacts/solid2/stage9/`, unless another artifact path is named.

Solid is the normal entry. React sources, adapters, compiler selection, compatibility
API/type exports and obsolete test setup are removed. Shared pure helpers and their tests
now live beside their final owners. Transport types are imported from their contract modules;
`SelectedAsset` belongs to the lightbox. The behavior inventory maps all 84 reference suites
to final tests, including replacement ownership/disposal checks.

Before cutover, both production builds passed entry/renderer isolation: 194 Solid and
216 React modules in `builds-precutover-final.log`. The full dual-renderer frontend run passed
141 files and 1224 cases after helper relocation and review fixes,
`frontend-precutover-final.log`.
React stayed runnable throughout the desktop comparisons.

The normal desktop comparison enables every relevant opt-in suite and runs the same
19 spec files serially for each renderer with fresh synthetic fixtures and disposable XDG
profiles. The migration-only shell/reference specs are excluded. Flags are
`MEDIATAGGER_GALLERY_PERF`, `MEDIATAGGER_GALLERY_SELECTION`, `MEDIATAGGER_GALLERY_BROWSING`,
`MEDIATAGGER_THUMBNAIL_FADE`, `MEDIATAGGER_UI_REDESIGN`, `MEDIATAGGER_TAG_EDITOR`,
`MEDIATAGGER_NATIVE_VIDEO`, `MEDIATAGGER_LIGHTBOX_IMAGES`, `MEDIATAGGER_LIGHTBOX_ACTIVITY`,
`MEDIATAGGER_ANIMATIONS`, `MEDIATAGGER_SETTINGS_LANGUAGE`, `MEDIATAGGER_SETTINGS_MAINTENANCE`
and `MEDIATAGGER_ABOUT`, all set to `1`.

Normal comparison: 117 distinct matching scenarios per renderer have passing latest results.
`desktop-react.log` had six failures, followed by `desktop-react-recheck.log` and
`desktop-react-animation-final.log`. `desktop-solid.log` had two idle-control UI timeouts.
The complete UI suite plus the narrow animation case then passed for both renderers in
`desktop-react-ui-recheck.log` and `desktop-solid-recheck.log`, 13 cases each.
`react-coverage.json`, `solid-coverage.json` and `coverage-parity.json` check exact case-name
sets and use the latest result of each case. The original failed invocations remain failures.

Reduced-motion comparison: 46 matching expected cases pass per renderer. Solid passed all
46 in one invocation, `reduced-solid.log`. React passed 45/46 in `reduced-react.log`; the unchanged
single-case fresh-profile recheck passed in `reduced-react-recheck.log`. The timeout was a
native Shift control-reveal sample; its cause has not been established. It remains an
intermittent verification limitation, not a claimed application fix.

The exact geometry matches in 12 gallery and 12 image-lightbox states across English/Polish,
light/dark themes and three widths. Query starts, page-request counts and scroll anchors
also match. `parity-comparison.json` records both sides; `react-evidence/` and `solid-evidence/`
retain the raw measurements and screenshots. Mounted gallery tiles remain bounded through
16 pages, eviction and return. Existing vocabulary, selection and measurement limits remain
asserted by the desktop and real virtual-core/model tests.

Same-host performance samples include WebDriver overhead and are diagnostic. React cold/warm
frame p95 was 49/28 ms and thumbnail p95 429/188 ms; Solid was 40/21 ms and 440/461 ms.
These samples do not establish a general speedup or a thumbnail regression. Deterministic
request, cache, DOM and measurement assertions remain the acceptance criteria.

Self-review and fixes:

- Restored startup popular-tag error logging. Runtime tests cover one fetch across repeated
  starts/view changes, failure, and ignored completion after disposal.
- Fixed lightbox runtime-error recovery. A new activation advances its error reset key;
  only failed views reset, so healthy navigation retains the media owner. The real-App
  regression verifies same-ID reopen, closing content and healthy navigation. Its initial
  incorrect aria-hidden assertion and corrected passing run remain in the evidence.
- Added no-selection mutation checks through the real lightbox model before removing the
  obsolete action-wrapper tests.
- About left Polish preferences for the next E2E spec. Preferences now reset after the
  native E2E window-title guard, while reloads within a spec retain preferences. A tooling
  test proves unexpected titles cannot mutate storage and unrelated keys remain intact.
- Real tag-page arrival changed dialog height during the animation sample. A proposed
  text-keyed comparison was rejected by cross-model audit because it could hide moving
  geometry. The original bounds assertion remains. The test delays delivery of the real
  response until after opening, restores invoke in finally, then waits for fixture content
  before closing. No tag data is mocked.
- Tag-editor fixture setup now removes a suggestion already assigned by the preceding
  scenario, retaining it on another asset to keep the vocabulary realistic.
- Animation/UI screenshot tests reveal idle floating controls through pointer/keyboard
  input before clicking. Their appearance, hit-target, duration and geometry assertions
  remain unchanged. Original failures and serial rechecks are retained.
- Cross-model review found reference assertions missing from the planned replacements.
  Restored exact ordered language options and five keyboard ownership/idle/fullscreen cases.
  Added five actual-runtime tests for CSV success/partial failure draining pending edits,
  cache invalidation, cleared-library ID reuse and bidirectional lightbox/bulk lock sharing.
  Retained CSV/ZIP export filename, title and filter contracts. The focused groups passed
  17 and 23 tests, followed by the full 1224-test dual-renderer run.
- Deferred narrowing the old array-or-descriptor filter signature until React removal,
  after the retained React caller correctly failed TypeScript. The final signature accepts
  the descriptor used by Solid.

Final verification:

- `MEDIATAGGER_UI_REDESIGN=1 MEDIATAGGER_GALLERY_BROWSING=1 MEDIATAGGER_LIGHTBOX_IMAGES=1 bash artifacts/solid2/stage9/run-all.sh --logLevel warn`:
  `bun run test:all` passed with exit 0. Quality includes all 11 locales, tooling tests,
  types, lint, formatting, rustfmt, Clippy and dependency audits. Frontend: 74 files and
  622 tests. Rust: 206 passed, two existing diagnostics ignored. Desktop: 39 passed cases,
  19 spec files without failures; other opt-ins were covered by the pre-cutover matrices.
  `test-all-final-recheck.log` and `final-suite.json`.
- The first final invocation, `test-all-final.log`, passed quality/frontend/Rust but had a
  group-order setup-hook timeout. Setup now waits for the lazy bulk panel and two stable
  animation frames before the real click. The focused recheck passed and then the complete
  pipeline passed. All selection, drag, persisted order and geometry assertions remain.
- `bun run test:build`: 193 production modules, Solid entry, no React or legacy feature
  trees. `build-final.log` and `artifacts/build/modules.json`.
- `PATH="$PWD/artifacts/tools:$PATH" bun run test:packaging`: 18 passed, `packaging-final.log`.
- `node --test --test-isolation=none e2e/build-e2e.test.js`: four passed,
  `e2e-build-final-explicit.log`. Default Node 26 output reported one file-level pass;
  the explicit run reports each build-order/profile/failure/preference assertion group.
- `node --test --test-isolation=none e2e/control-support.test.js`: six passed,
  `control-final.log`. This includes local sockets and owned-process cleanup.
- `PATH="$PWD/artifacts/tools:$PATH" bun run privacy:check`: passed for full history,
  staged objects and tracked working files, `privacy-staged.log` and `privacy-final.log`.
  The refreshed index and final working files pass.
- `git diff --cached --check`: passed after preserving existing line endings and removing
  stray carriage returns from changed import lines.

The lockfile removes packages only. Test-tooling transitive `react-is` packages remain under
Testing Library/WebdriverIO formatters; they are absent from the production module graph.
Final production gallery/lightbox geometry exactly matches all 24 pre-cutover states,
`final-geometry.json`. The final screenshots were inspected. Owned desktop processes exited.

Desktop evidence establishes Linux X11/WebKit behavior on this host with software-rendered
libmpv. It does not establish other operating systems, hardware decoding, audible output,
or a published release package. Native file chooser interactions remain covered through
mocked frontend boundaries; desktop CSV/bundle tests use the real commands directly.
The two existing Rust diagnostic tests remain ignored. Rust advisory exclusions are unchanged.

Staged source, tests, configuration and documentation were reviewed for scope and private
content. No media, screenshots, raw logs, databases or generated bundles are staged.
Cross-model review by `gpt-6-astra` found the coverage and setup issues recorded above;
all findings were fixed and rechecked. Same-host timing samples and the isolated React
reduced-motion timeout remain limitations, not performance or cross-platform guarantees.

Local commit subject: `refactor: complete Solid migration and remove React`.
Local commit hash: recorded when stage 10 starts.

Next stage: [documentation](10-documentation.md). Current build, entry and IPC guidance is
corrected here; the full canonical frontend/subsystem documentation rewrite belongs to stage 10.
