# Solid 2 migration progress

## Current state

Stage 1 is complete. React remains the default and functional reference.
Solid has its own entry, compiler, TypeScript context, and browser-runtime test project.

- Working branch: `feature/rewrite-react-to-solid2`.
- React reference commit: `f11126dfafd40f877ecf5e496abc37d77a435e78`.
- Stage 1 starting commit: `0e1cc02`, the migration plan.
- Next implementation task: [stage 2](02-platform-and-shell.md).
- No worktrees, GitHub pushes, or merges to `main` are permitted.

## Stage tracker

Statuses: `Not started`, `In progress`, `Blocked`, `Ready to commit`, `Complete`.
A stage becomes `Complete` only after its tests, self-review, fixes, and local commit.
Use this table as the status source of truth; the stage files describe planned work.

| Stage | Status | Tests | Self-review | Local commit |
| --- | --- | --- | --- | --- |
| [1. Reference and toolchains](01-reference-and-toolchains.md) | Complete | 703 frontend, 206 Rust, both builds, desktop and packaging verified | Findings fixed and rechecked | `build: add isolated Solid 2 migration setup`; hash recorded at stage 2 |
| [2. Platform and shell](02-platform-and-shell.md) | Not started | Not run | Pending | None |
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
The next stage should record the resulting hash here.

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
