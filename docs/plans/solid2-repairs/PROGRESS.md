# Solid 2 repair progress

Status: all fourteen batches complete and verified; all twenty confirmed findings fixed.
Current batch: none active.
Next action: none in this repair plan. Changes remain uncommitted for review.

| Batch | Status | Evidence |
| --- | --- | --- |
| 01 | complete | DR-001 and DR-002 fixed; [mutation, control and Cargo evidence](../../../raports/20261011-batch-01/summary.json) |
| 02 | complete | DR-020 fixed; production bootstrap/storage regressions and [original-source comparison](../../../raports/20261011-batch-02/summary.json) |
| 03 | complete | DR-003 fixed; shared native completion, runtime regressions, mutation check and isolated desktop remounts |
| 04 | complete | DR-004 and DR-005 fixed; reversed admission/replies, capacity regression, mutation control and isolated desktop queries |
| 05 | complete | DR-006, DR-013 and DR-015 fixed; deferred Settings/save regressions and favorite ownership mutation check |
| 06 | complete | DR-010 and DR-011 fixed; original-source recovery regression, literal-name modal tests and 12 native desktop checks |
| 07 | complete | DR-012 and DR-014 fixed; prefetch/page Retry regressions, late-load ownership checks and two native WebView scenarios |
| 08 | complete | DR-008 fixed; integrated range/banner regressions, original-source comparison and two native desktop scenarios |
| 09 | complete | DR-022 fixed; full four-case desktop suite, per-sample file provenance and reordered-suite failure |
| 10 | complete | DR-007 and DR-009 fixed; bounded native layout, scrollbar/wheel/selection, original-source failure and corrected gallery contract |
| 11 | complete | DR-016 fixed; session-scoped native Escape, layer/deletion guards, 7 private-display input cases |
| 12 | complete | DR-018 fixed; controlled Stop replies, terminal/progress preservation, new-render pending ownership and rendered controls |
| 13 | complete | DR-019 fixed; 153 translations, 11-locale rendered regression, original-resource failures and EN/DE desktop inspection |
| 14 | complete | Nine combined checks and ten native scale checks passed; follow-up fixed stationary-pointer input and passed all 44 desktop cases in one run |

## Planning verification, 2026-10-11

Starting HEAD: `6fd951d2e7fd38e643223cd8c7582e2693e6fd66`. The working tree was clean.

`python3 raports/20261010T182819Z-solid2-deep-review/verify_stage15.py` passed with no errors. It checked 693 verifier hash entries across 224 sources and 109 preserved original artifacts. The review record remains complete with 70 completed assignments, 32 answered handoffs, 20 confirmed findings and two rejected findings.

The repair sequence uses the final catalogue's triggers, repair directions, counterexamples and limits. The first batch was additionally checked against the current runtime test, runtime, real tags adapter, scheduling helper, verifier notes, package scripts and testing/architecture contracts.

Plan validation assigned all 20 confirmed findings exactly once across 13 repair batches, followed by one combined verification batch. Both rejected findings remain excluded. All local links across the plan and progress documents resolve, including heading anchors. `git diff --check` and whitespace checks for the new files passed. The local validation result is [repair-plan-check.json](../../../raports/20261010T182819Z-solid2-deep-review/evidence/repair-plan-check.json).

Application tests, builds and desktop checks did not run during planning. No fixes have been applied. The original review's completed status remains unchanged; this file owns future repair status.

## Batch 01 verification, 2026-10-11

Started at `be27032bacb5763e79af171ae4fecb64e0c86708` with a clean working tree. Work used the current checkout. No worktrees, commits or pushes were created. The fourteen-batch sequence is unchanged.

| Finding | Disposition | Evidence |
| --- | --- | --- |
| DR-002 | Fixed. The disposal test calls the real tags adapter, instruments its returned promise, and captures completion of the runtime's success handler. After disposal and deferred native resolution, it awaits that completion and flushes Solid through `settle`. Existing `afterEach` restores the adapter and promise spies. Startup idempotence and failure coverage remain intact. | Baseline and strengthened focused suite each passed 3/3. Removing only the successful-response disposal guard in memory produced exactly one failing test, expected `[]`, received `["late"]`; the other two passed. Guard-preserving control passed 3/3. |
| DR-001 | Fixed. The testing guide's matrix and script description cover all Cargo-discovered integration targets, distinguish the focused scripts, and explain explicit opt-in for ignored diagnostics. No fixed target count is part of the guide's contract. | Compared `package.json`, `src-tauri/Cargo.toml`, locked offline Cargo metadata and `#[ignore]` declarations. Metadata discovered `backend_e2e`, `backend_integration`, `backend_runtime`, `tag_save_benchmark`, `tag_vocabulary`, `thumbnail_bulk` and `thumbnail_clear`. Ignored diagnostics are `metadata_scale_measurements` and `tag_save_benchmark`. |

The instrumentation distinguishes the runtime's success-only `.then` from Vitest's own promise observer, which supplies both fulfillment and rejection callbacks. It requires exactly one runtime handler before resolving the deferred response. It uses no sleeps, fixed microtask counts or already-satisfied `waitFor` assertion.

Commands ran from the repository root:

```sh
# Before and after the test change: exit 0, 3/3 tests each.
bun run test -- src/app/__tests__/runtime.test.ts

# Reproducible probe and evidence checks: exit 0.
python3 raports/20261011-batch-01/verify.py

# The verifier runs these discovery and execution commands in this order.
# Each discovery returned exactly src/app/__tests__/runtime.test.ts.
DR002_MUTATE=1 node node_modules/vitest/vitest.mjs list --config raports/20261011-batch-01/probe.config.ts --filesOnly --json
# Expected exit 1: disposal assertion fails with ["late"], other tests pass.
DR002_MUTATE=1 node node_modules/vitest/vitest.mjs run --config raports/20261011-batch-01/probe.config.ts --reporter=json --outputFile=raports/20261011-batch-01/mutation-results.json
DR002_MUTATE=0 node node_modules/vitest/vitest.mjs list --config raports/20261011-batch-01/probe.config.ts --filesOnly --json
# Exit 0: all three tests pass.
DR002_MUTATE=0 node node_modules/vitest/vitest.mjs run --config raports/20261011-batch-01/probe.config.ts --reporter=json --outputFile=raports/20261011-batch-01/control-results.json

cargo metadata --manifest-path src-tauri/Cargo.toml --locked --offline --no-deps --format-version 1
rg -n -A3 '#\[ignore' src-tauri/tests
bun run typecheck
bun run lint
git diff --check
```

Typecheck and lint passed, with lint checking 254 files and applying no fixes. `git diff --check` passed. Changed documentation links and anchors were checked locally.

The ignored evidence directory is [`raports/20261011-batch-01/`](../../../raports/20261011-batch-01/). It contains the runnable `verify.py` and `probe.config.ts`, exact subprocess commands in `commands.json`, focused-suite logs, discovery outputs, JSON test reports, transform inputs/outputs and hashes, Cargo metadata, ignored declarations, typecheck and lint logs. The probe inherits the normal Vitest configuration and replaces its include list directly. Both runs require exactly one matching success guard; the mutation replaces it once and the control preserves it. Input/output comparison confirms no other in-memory change. Before/after hashes of frontend and backend source confirm unchanged on-disk bytes during the probe.

The completed review and its historical hashes remain untouched. A read-only comparison against those hashes records intentional drift only in `src/app/__tests__/runtime.test.ts` and `docs/development/testing.md`, in `historical-drift.json`. The historical validator was not rerun because it rewrites evidence; the probe separately checked that the historical evidence files remained unchanged.

Verification is limited to the focused Vitest suite under jsdom with mocked native responses, static checks, and Cargo discovery. Backend tests, desktop execution and builds were unnecessary for this test/documentation batch and did not run. No live IPC, native startup or desktop teardown behavior was verified. Public APIs, IPC contracts, dependencies and production behavior are unchanged.

Batch 01 stops here. Resume with batch 02, DR-020; no preference-storage changes have been made.

## Batch 02 verification, 2026-10-11

Started at `be27032bacb5763e79af171ae4fecb64e0c86708` with the completed batch 01 changes
still uncommitted in `runtime.test.ts`, the testing guide and repair plan/progress. Those
changes remain intact. Work used the current checkout without worktrees, commits or pushes.

DR-020 is fixed. [i18n core](../../../src/i18n/core.ts) and
[theme](../../../src/ui/dom/theme.ts) catch storage-property and method failures at their
read/write boundaries. Failed language reads fall back to browser language, then English;
failed theme reads use dark. Failed writes leave i18next events, reactive settings state,
DOM attributes and native theme synchronization active. The existing bootstrap and runtime
callers use these owners unchanged. Ordinary preference persistence remains intact.
The frontend ownership and localization guides now describe this contract.

[Preference bootstrap regressions](../../../src/app/__tests__/preferenceStorage.test.tsx)
evaluate the real core under each fault, then run `mountApplication` with its default
readiness promise and the real App. Application modules reset between cases; the real
Solid runtime stays shared with Testing Library, and i18next listeners are cleared between
initializations. No core, App, theme or readiness implementation is replaced. Native calls
are mocked. Nine cases cover property denial, read denial, write denial, quota failure,
theme-only failures, supported/unsupported browser fallback and working storage. Each
checks the mounted gallery, initial preferences, changes through Settings, translated UI,
DOM attributes and retained in-memory selection after leaving and reopening Settings.
The working-storage control also checks normalized initial and changed values in storage.
The [theme test](../../../src/ui/dom/__tests__/theme.test.ts) checks that a failed write
still reaches the native theme adapter.

Verification from the repository root:

```sh
# Before production edits: expected exit 1, eight failures and one passing control.
# After edits: exit 0, nine tests passed.
bun run test -- src/app/__tests__/preferenceStorage.test.tsx

# Final regressions: exit 0, four files and 30 tests passed.
bun run test -- src/app/__tests__/preferenceStorage.test.tsx src/app/__tests__/shell.test.tsx src/app/__tests__/bootstrap.test.tsx src/ui/dom/__tests__/theme.test.ts

# Final tests against original core/theme supplied by an in-memory Vite transform.
# Verifier exits 0 only for exactly eight failures and one passing control.
python3 raports/20261011-batch-02/verify.py

# Both exit 0. Build includes both TypeScript checks and the Vite production build.
bun run build
bun run lint

git diff --check
```

The original-source probe first discovers exactly the new preference test file. It reads
core/theme from the starting commit into ignored fixtures and substitutes them only during
Vite transformation. The final test file fails eight fault cases while its ordinary-storage
control passes. Before/after hashes confirm unchanged on-disk production source. The initial
probe config hit a sandbox `spawnSync git EPERM`; exporting fixtures through Python removed
that dependency from the config. An initial discovery assertion was corrected to read
Vitest's object entries. The original first test run also exposed duplicate Solid loading;
explicitly importing the shared runtime before module resets removed that warning from the
final tests and original-source probe.

Raw logs, the runnable probe, JSON results and commands are in
[`raports/20261011-batch-02/`](../../../raports/20261011-batch-02/).
The read-only comparison with DR-020's historical hashes records intentional drift in core,
theme, theme tests and the two owning guides, plus the testing-guide change from batch 01.
Historical reports, verifiers and hashes remain untouched.

Verification covers injected `SecurityError` and `QuotaExceededError` exceptions under
jsdom, actual frontend mounting and settings interaction, plus a mocked native adapter
assertion. It does not measure actual browser quota exhaustion, native storage denial or
its incidence. Desktop execution, live IPC, Rust tests, Wayland and release packaging were
not run. The combined full-suite checks remain scheduled for batch 14. No IPC or dependency
changes were needed. Next is batch 03, DR-003.

## Batch 03 verification, 2026-10-11

Started at `be27032bacb5763e79af171ae4fecb64e0c86708` with batches 01 and 02 still
uncommitted. Their source and documentation changes remain intact. Work used the current
checkout without worktrees, commits or pushes. The recorded trigger still existed in
`StartupScanState::take_roots`: a second command received an empty root list while the
first scan continued, then returned null without a completion refresh.

DR-003 is fixed. [Startup state](../../../src-tauri/src/app/state.rs) now distinguishes
pending roots, running work with observers, and a retained result. The
[command](../../../src-tauri/src/commands/scan.rs) starts one detached process-owned task,
runs the scan on the blocking pool under the existing scan lock, and publishes success or
failure to current and later callers. Dropping the initiating observer does not abandon
publication. Worker join errors are retained too. Only a launch with no enabled roots
returns null. Preference changes still affect the next process, and failure never retries
automatically. `ScanSummary` gains `Clone`; its serialized fields and command arguments
are unchanged. The scanning adapter documents the new null and replay semantics.

The [scan model](../../../src/features/settings/createScanSettings.ts) skips completion
follow-ups after disposal. Active owners refresh query, vocabulary and roots after a
summary or failure. A later mount can perform an additional refresh from a retained
result; it does not start another scan. System lifecycle, scanning, settings, IPC and
testing guides describe this contract.

[Runtime regressions](../../../src/app/__tests__/startupScanRemount.test.ts) await the
actual startup action across disposal and replacement with 0, 3 and 160 initial rows.
They assert refreshed totals without progress events or uncached page requests, retained
completion on a later mount, the null/no-work control, committed rows after failure and
no disposed-owner root refresh. The native tests cover enabled-root snapshot contention,
awaiting a running command, abandonment of the first caller, retained success/error,
worker panic and a no-work launch. Existing preference, manual scan and API tests remain.

Verification from the repository root, all exit 0 unless noted:

```sh
# Initial focused run: three files, 23 tests.
bun run test -- src/app/__tests__/startupScanRemount.test.ts src/features/settings/__tests__/startupScan.test.tsx src/platform/tauri/__tests__/api.test.ts

# Final focused run: six files, 57 tests.
bun run test -- src/app/__tests__/startupScanRemount.test.ts src/app/__tests__/runtime.test.ts src/app/__tests__/shell.test.tsx src/features/settings/__tests__/startupScan.test.tsx src/features/settings/__tests__/createSettingsActions.test.tsx src/platform/tauri/__tests__/api.test.ts

# Final native runs: 11 and six tests respectively, with three tests shared.
cargo test --manifest-path src-tauri/Cargo.toml --locked --offline --lib startup
cargo test --manifest-path src-tauri/Cargo.toml --locked --offline --lib commands::scan::tests

# Verifier passes only when the mutation fails and restored production passes.
python3 raports/20261011-batch-03/mutation.py

# Creates private Xvfb/D-Bus and temporary XDG directories, enables the remount spec,
# builds fresh frontend/native E2E artifacts and runs both startup specs.
python3 raports/20261011-batch-03/desktop.py

bun run typecheck
bun run lint
bun run rust:clippy
bun run rust:fmt
git diff --check
```

The native mutation replaces exactly one running-observer branch with `Ok(None)`, restoring
the finding's old response behavior. The focused command test fails with
`running startup scan must remain observable`, exit 101. The verifier restores the source
bytes in `finally`, checks their SHA-256, and runs the same test successfully, exit 0.
The mutation is a temporary on-disk edit serialized after desktop builds, not a change to
historical review evidence. The final native and frontend runs use restored production.

The two desktop specs passed all three cases. The existing spec covers selected folders,
next-process preference changes, no repeated scanning and manual Rescan all. The new
[remount spec](../../../e2e/specs/startup-scan-remount.e2e.js) uses a temporary copy of the
E2E binary with a gated ffprobe wrapper beside it. Actual video probing remains blocked
while the WebView reloads. Releasing it changes the visible count from 0 to 1 and 2 to 3
without navigation or a manual refresh. Each case checks one gated probe invocation,
no gate timeout, a retained summary and another reload after completion. The wrapper
runs the real ffprobe; no IPC response or production scan implementation is mocked.
Both screenshots were inspected and show the expected synthetic video/image tiles and
counts. This proves scan completion and gallery publication, not native video playback.

Logs, mutation/control results, commands and the read-only historical hash comparison
are in [`raports/20261011-batch-03/`](../../../raports/20261011-batch-03/).
Desktop screenshots and result JSON are in
[`artifacts/startup-scan-remount/`](../../../artifacts/startup-scan-remount/).
Cleanup verification found no remaining app, driver or Xvfb processes, no remount fixture
directories, and confirmed removal of the temporary XDG root. Desktop logs contain host
portal/accessibility/Mesa warnings; they did not fail the tested scan/gallery paths.

Historical reports and hashes remain unchanged. The read-only comparison records expected
source/documentation drift from this batch and earlier repairs. Full combined suites,
Wayland and release packaging were not run and remain outside this batch's evidence.
Live scan failure was not injected on desktop; failure/replay and worker panic are covered
by the focused native/frontend tests. No dependencies changed. Next is batch 04.

## Batch 04 verification, 2026-10-12

Started at `be27032bacb5763e79af171ae4fecb64e0c86708` with batches 01 through 03
still uncommitted. Those changes remain intact. Work used the current checkout without
worktrees, commits or pushes. The installed Tauri command macro calls async commands
inside the spawned response future, confirming that registration inside the command
can run out of frontend call order. Capacity insertion still returned the same false
value as supersession before this repair.

DR-004 is fixed. The [query model](../../../src/library/query/createAssetQuery.ts)
creates one UUID per owner and sends it through the
[adapter](../../../src/platform/tauri/assets.ts) and
[command](../../../src-tauri/src/commands/assets.rs). The
[query manager](../../../src-tauri/src/services/asset_query_service.rs) compares each
owner's generation under a mutex before assigning a process-wide request token.
Equal or lower generations cannot replace the accepted request. New owners have
independent counters, including after remount. Cross-owner supersession remains possible;
a still-current frontend start now exposes it through the existing query error/Retry
path. Obsolete local responses remain ignored. No automatic retry loop was added.
Owner high-water marks remain for the process lifetime, one entry per query-model owner;
they are separate from the retained-ID budget.

DR-005 is fixed. Session insertion returns an error when the authoritative ID vector
exceeds the memory budget. Token/epoch mismatches remain superseded. The error asks the
caller to narrow the search; the UI uses its existing translated query failure banner
and Retry. The frontend does not branch on error wording. The 64 MiB production budget,
four-session LRU, TTL and query dependency invalidation remain in place. Rejection occurs
before eviction, and a later fitting query can page and resolve positions normally.
The query/gallery, database and IPC guides now describe both outcomes. Direct desktop
workflow calls and backend restore tests pass the new owner argument.

Regressions and exact commands from the repository root:

```sh
# Before the capacity repair: exit 101, expected Superseded instead of an error.
cargo test --manifest-path src-tauri/Cargo.toml --locked --offline --lib authoritative_capacity_failure

# Before current-supersession handling: exit 1, two expected failures and 15 passes.
bun run test -- src/library/query/__tests__/createAssetQuery.test.ts

# After repair: exit 0, 16 native query-service tests.
cargo test --manifest-path src-tauri/Cargo.toml --locked --offline --lib services::asset_query_service::tests

# Exit 0 only when removing generation rejection fails and restored production passes.
python3 raports/20261012-batch-04/mutation.py

# Exit 0, 12 files and 125 frontend tests.
bun run test -- src/library src/app/__tests__/gallery.test.tsx src/platform/tauri/__tests__/api.test.ts

# Exit 0, all 192 Rust library tests, including restore and command tests.
cargo test --manifest-path src-tauri/Cargo.toml --locked --offline --lib

bun run typecheck
bun run lint
bun run rust:clippy
bun run rust:fmt
git diff --check
```

The native admission test registers generation 99 before 98, verifies the latest filter's
usable session, rejects the obsolete token both before and after that result, then admits
a new owner's generation 1. Existing supersession, clear, revision, eviction and cache-hit
controls also pass. The mutation disables exactly one generation rejection predicate;
the regression fails because the older request becomes Ready. The script restores exact
source bytes in `finally`, checks the hash, and runs the same test successfully. Builds
and native tests were serialized around this temporary mutation.

The capacity regression uses 32 synthetic database rows and a test-local 64-byte budget.
It confirms explicit rejection with an unchanged latest token and cache epoch, then narrows
to two tagged rows and checks page and position results. Frontend tests cover both reply
orders, owner identity across refresh/remount, current supersession followed by Retry,
and capacity failure in empty and previously populated App views. Narrowing the filter
removes the rendered error and updates the count without automatic retries. Adapter tests
check `ownerId`, generation and propagation of the rejected string. Initial component
assertions were corrected for the existing `1 items` toolbar text and duplicate toolbar/
footer counts; the final tests use the toolbar selector.

Desktop verification used [verify-mediatagger](../../../.cursor/skills/verify-mediatagger/SKILL.md)
and `bun run app:control start`, which built fresh frontend and native E2E artifacts.
The sandbox initially denied local sockets with EPERM; escalation allowed the isolated
controller to run. Doctor verified the private display, `Tagrove E2E` title, temporary
profile and sole synthetic media root. Session `920708dd39c4ebc5dc8dfe5efa410056`:

```sh
bun run app:control doctor 920708dd39c4ebc5dc8dfe5efa410056
bun run app:control fill 920708dd39c4ebc5dc8dfe5efa410056 .filter-input absent-batch04-tag
bun run app:control keys 920708dd39c4ebc5dc8dfe5efa410056 Enter
bun run app:control inspect 920708dd39c4ebc5dc8dfe5efa410056
bun run app:control screenshot 920708dd39c4ebc5dc8dfe5efa410056
bun run app:control click 920708dd39c4ebc5dc8dfe5efa410056 'button[aria-label="Clear all search filters"]'
bun run app:control inspect 920708dd39c4ebc5dc8dfe5efa410056 .workspace-count
bun run app:control refresh 920708dd39c4ebc5dc8dfe5efa410056
bun run app:control inspect 920708dd39c4ebc5dc8dfe5efa410056 .workspace-count
bun run app:control screenshot 920708dd39c4ebc5dc8dfe5efa410056
bun run app:control logs 920708dd39c4ebc5dc8dfe5efa410056
bun run app:control stop 920708dd39c4ebc5dc8dfe5efa410056
bun run app:control stop 920708dd39c4ebc5dc8dfe5efa410056
```

Search displayed No results and 0 items; clearing restored all seven fixtures; reloading
created a new frontend owner and again displayed seven items. Both screenshots were
inspected. The controller, display and driver process groups are gone, the temporary root
was removed, and evidence remains under
[`artifacts/app-control/920708dd39c4ebc5dc8dfe5efa410056/`](../../../artifacts/app-control/920708dd39c4ebc5dc8dfe5efa410056/).
Logs include one ResizeObserver undelivered-notifications window error and host graphics/
keyboard warnings. The observed query actions succeeded; no claim of warning-free desktop
execution is made.

Raw test logs, mutation/control evidence, cleanup checks and read-only historical hash
comparison are in [`raports/20261012-batch-04/`](../../../raports/20261012-batch-04/).
Historical review evidence and hashes remain unchanged. Their comparison records expected
drift in changed source/docs and earlier batches. Typecheck, lint, Clippy, formatting and
diff checks passed. The desktop check proves the new owner payload and remount behavior
through live IPC. Admission inversion and capacity failure were controlled in native/
frontend tests, not forced in the desktop process. Full Rust integration binaries, the
combined frontend suite, Wayland and release packaging were not run in this batch.
Combined verification remains batch 14. Next is batch 05.

## Batch 05 completed, 2026-10-12

Started in the existing checkout at `be27032bacb5763e79af171ae4fecb64e0c86708`,
with earlier repair batches already modified. Those changes were preserved. No worktree,
commit or push was created.

- DR-006 fixed in `createDetailsCache.ts` and `createMutations.ts`. Each write captures
  the details-cache generation; restoring its full record requires the same generation.
  Canonical metadata publication remains separate. Runtime integration regressions hold
  tag saves across actual Settings duplicate-rename and rescan controllers with mocked IPC.
  After their query refresh and delayed save, reopening fetches current path/size and keeps
  saved tags. Normal-save reuse, identity reset, eviction before save, stale reads and
  disposal countercases pass in the metadata tests.
- DR-013 fixed in `createMutations.ts`. Favorite persistence completes independently of
  its follow-up query refresh. The lightbox regression removes a favorite under Favorites-only,
  rejects the refresh, observes the saved value with navigation/query failure and no save
  failure, then retries the query without an inverse write. A genuine write rejection still
  sets the save-error state. A deferred-refresh test proves save completion does not wait
  for query completion.
- DR-015 fixed in `createLightbox.ts`. Pending favorite admission, selection presentation
  and completion cleanup use the identity epoch. Tests cover close, identity maintenance,
  same-ID reopen, and old success/rejection while a new-epoch write remains pending.
  The maintenance test delays the caller continuation after the real mutation releases
  its backend-write accounting; the direct-reset cases hold the actual mocked IPC reply.
  These checks still pass after DR-013 removes the original awaited-refresh lifetime.

Before production edits, the three focused files reported 7 failures and 55 passes.
The failures showed stale full records after rename/rescan, a false favorite save error,
retired pending state on reused IDs, and a favorite save still waiting for refresh.
The final expanded run passed 158 tests across 10 files. An additional in-memory transform
removed only the favorite completion cleanup guard, requiring exactly one match. Both
new-epoch pending cases then failed on reopen; the guard-preserving control passed both.
The transform never changed application files and selected only the lightbox test file.

Commands and results:

```sh
# Initial baseline: exit 1, 7 failed and 55 passed before production edits.
bun run test -- src/app/__tests__/runtimeIntegration.test.ts src/library/metadata/__tests__/mutations.test.ts src/features/lightbox/__tests__/createLightbox.test.ts

# Final expanded regression run: exit 0, 158/158 tests in 10 files.
bun run test -- src/app/__tests__/runtimeIntegration.test.ts src/library/metadata src/features/lightbox/__tests__/createLightbox.test.ts src/features/lightbox/__tests__/LightboxModal.test.tsx src/library/query src/features/settings/__tests__/createSettingsActions.test.tsx src/features/settings/__tests__/createSettingsActionsConfirmFlow.test.tsx

# Control: exit 0, 2 passed and 34 skipped. Mutation: expected exit 1, 2 failed and 34 skipped.
bun run test -- --config raports/20261012-batch-05/ownership.config.mjs
MUTATE_FAVORITE_CLEANUP=1 bun run test -- --config raports/20261012-batch-05/ownership.config.mjs

# All exit 0. Build includes production module-graph validation.
bun run typecheck
bun run lint
bun run test:build
git diff --check
```

Updated the query/cache, lightbox, Settings and maintenance-safety docs. Raw logs, the
cleanup-guard probe and the read-only comparison with historical verifier hashes remain
under [`raports/20261012-batch-05/`](../../../raports/20261012-batch-05/).
Historical evidence is unchanged; its hashes show expected drift in repaired source/docs.
The passing mutation control overlaps the 158-test run and is not an additional unique count.

The regressions exercise frontend owners and real Tauri adapters with synthetic mocked
replies. No native filesystem rename, rescan, database import, desktop input, Wayland or
release packaging was executed in this batch. No Rust or IPC contract changed. Combined
frontend/backend and desktop verification remains batch 14. Next is batch 06.

## Batch 06 completed, 2026-10-12

Started at `be27032bacb5763e79af171ae4fecb64e0c86708` in the existing checkout with
batches 01 through 05 uncommitted. Earlier changes remain intact. No worktree, commit
or push was created. Both recorded triggers remained in the owning source.

- DR-010 fixed in [SearchTagator](../../../src/features/search/SearchTagator.tsx) and
  [createSuggestions](../../../src/features/search/suggestions/createSuggestions.ts).
  Failure belongs to the request context and vocabulary snapshot, independently of focus.
  Blur still cancels pending reads and closes results, while Retry survives focus transfer.
  Activation restores input focus and requests suggestions again. Draft/vocabulary changes
  invalidate obsolete failure presentation; disabled inputs hide Retry. Shared TagEditor
  callers inherit this behavior without separate focus handlers.
- DR-011 fixed in [tagListSelectionService](../../../src/features/search/tag-list/tagListSelectionService.ts)
  and [TagListModal](../../../src/features/search/tag-list/TagListModal.tsx). Selection maps
  store normalized names and display values. Lookup, copying, removal, summaries and filter
  serialization use Map operations, including for `constructor` and `__proto__`.
  Existing include/deselect, exclusion and literal serialization controls still pass.

The six added rendered-modal regressions fail against the original dictionaries because
both names initially appear selected. After repair, single-click/Apply, double-click
exclusion and Shift+Enter exclusion emit the expected literal filters for both names.
The recovery tests use the real SuggestionClient with one failed worker construction,
then the shared in-process worker transport. Pointer down/up and Tab/Enter recover a
rendered `cat` option in search and both shared editor variants, restore focus and retain
ordinary Enter submission. Existing stale-result and disposal controls pass; the new model
check verifies failure survives blur without another request and expires on draft/vocabulary
changes. Final focused verification passed 90 tests in 14 files.

The original-source probe substitutes the pre-repair SearchTagator and createSuggestions
only during Vite transformation. It discovers one executed test file with six cases: all six
fail against original source and pass with current source. SHA-256 comparison confirms the
probe leaves application files unchanged. Early test drafts used an incorrect translated
label, split pointer events across separate user-event sessions and queried highlighted
text as an unsplit accessible name. Those assertions were corrected before the final probe.

Commands from the repository root:

```sh
# Exit 0, 90 tests in 14 files.
bun run test -- src/features/search src/features/tagging src/utils/__tests__/literalTagSearch.test.ts

# Exit 0 only if original source fails all six and current source passes all six.
python3 raports/20261012-batch-06/verify-original.py

# Private Xvfb/D-Bus, disposable XDG profile, fresh E2E build, opt-in desktop spec.
# Exit 0, 12 checks. The runner sets MEDIATAGGER_SEARCH_RECOVERY=1 and
# MEDIATAGGER_NATIVE_DISPLAY to its private display.
python3 raports/20261012-batch-06/desktop.py

bun run typecheck
bun run lint
bun run test:build
git diff --check
```

The desktop [search-recovery spec](../../../e2e/specs/search-recovery.e2e.js) seeds two
copied PNG fixtures and three tags through live IPC. For each editor and input method,
it faults the first Worker construction, confirms the fault occurred, activates the visible
Retry and observes `cat` from the real bundled worker. Pointer checks use the guarded native
XTest helper; keyboard checks use Tab/Enter with actual window focus. Six further checks
exercise literal-name selection in the real modal and assert both filter text and one matching
asset. Search-pointer, lightbox-keyboard and bulk-pointer screenshots were inspected.

The first three desktop attempts passed all six tag-selection checks but never reached
worker failure. WebDriver had an active DOM input without native window focus. Establishing
focus with the repository's XTest helper made the fault and recovery observable. Earlier logs
remain alongside the passing run. Sandbox socket restrictions required escalation; port
preflight also needed SO_REUSEADDR for TIME_WAIT after stopped runs. No active listener was
terminated. Host accessibility/portal warnings remain in logs.

Search/tag, frontend focus and testing docs describe the final behavior. Raw results,
original-source probe, historical hash comparison and cleanup evidence are under
[`raports/20261012-batch-06/`](../../../raports/20261012-batch-06/); screenshots remain in
[`artifacts/search-recovery/`](../../../artifacts/search-recovery/).
All four temporary XDG roots and synthetic media roots were removed. Process inspection
found no remaining Tauri app, driver or Xvfb processes. Historical review hashes are unchanged;
the read-only comparison records expected source/documentation drift.

The native failure is injected, not an incidence measurement. This batch covers Linux X11
WebKitGTK, not Wayland, release packaging or speech output. No Rust, IPC or dependency
contract changed. Full combined suites remain batch 14. Next is batch 07.

## Resume rules

Begin with the next pending batch, inspect current changes, then record its start here. Before marking a finding fixed, record the regression result against broken and repaired behavior, exact commands, relevant source changes and verification limits. Keep raw evidence ignored. Do not update the historical review hashes after intentional repairs.


## Batch 07 verification, 2026-10-12

Started at `be27032bacb5763e79af171ae4fecb64e0c86708` with batches 01 through 06
still uncommitted. Existing changes remain intact. Work used the current checkout;
no worktree, commit or push was created.

DR-012 is fixed in [the lightbox model](../../../src/features/lightbox/createLightbox.ts)
and [query model](../../../src/library/query/createAssetQuery.ts). Failed target reads now
publish the existing localized navigation error and Retry action. The model retains the
failed global index, and `retryAssetAt` clears only that page's failure before reading it
again. A successful retry selects the target in the same query session. Other failed pages
stay blocked. Cached navigation remains available, repeated failure remains retryable, and
selection, close, query changes and disposal invalidate pending publication. Position retry
retains its query-refresh behavior.

DR-014 is fixed in [image controls](../../../src/features/lightbox/image/createImageControls.ts)
and [the media stage](../../../src/features/lightbox/LightboxMediaStage.tsx). Loads may update
natural dimensions only for the current image under active, undisposed controls. The stage
also rejects callbacks from retired asset-ID/kind/path activations. A late event cannot
replace the current transform target. Current-image loads still update fit normally.
The lightbox, gallery/query and frontend ownership guides document these contracts.

The nine new lightbox/image regressions first ran against unchanged batch-start production:
9 failed and 41 existing tests passed. After the fixes, all 50 passed. The final affected
frontend run, including the additional target-only query retry regression, passed 162 tests
across 11 files. These runs overlap and are not separate unique totals. Cases cover failed
adjacent prefetch at the 128-item boundary, cached Previous, repeated failure and successful
Retry without a new query, stale Retry after selection/close/refresh/disposal, other failed
pages staying blocked, late loads after ID or path replacement, normal current-image load,
zoom/pan after late delivery, and inactive/disposed owners.

Verification from the repository root:

```sh
# Before fixes: expected exit 1, nine failures and 41 passes.
# After fixes: exit 0, 50 tests passed.
bun run test -- src/features/lightbox/__tests__/createLightbox.test.ts src/features/lightbox/__tests__/imageControls.test.tsx

# Final affected layer: exit 0, 11 files and 162 tests passed.
bun run test -- src/features/lightbox src/library/query/__tests__/createAssetQuery.test.ts src/app/__tests__/lightboxRecovery.test.tsx

# Exit 0. Lint checked 260 files with no fixes or diagnostics.
bun run typecheck
bun run lint

# Private Xvfb/D-Bus session and disposable XDG profile, as recorded in run-desktop.sh.
# Exit 0, two desktop cases passed. The harness also passed typecheck, Vite build
# and the isolated Tauri debug build.
MEDIATAGGER_LIGHTBOX_RECOVERY=1 bun run test:e2e:tauri --spec e2e/specs/lightbox-recovery.e2e.js

git diff --check
```

The [desktop spec](../../../e2e/specs/lightbox-recovery.e2e.js) indexes 132 copied synthetic
PNGs. An injected negative offset produces a real native argument rejection for page 128.
Cached Previous works; restoring the payload and clicking visible Retry issues exactly one
fresh page request and selects index 128. A second case navigates from image A to loaded B,
then loads a synthetic 40×2000 SVG on detached A through WebKit. B's fitted dimensions,
zoom and transform remain unchanged; subsequent zoom and reset still affect B. Saved
screenshots were inspected and show the expected synthetic image. This tests controlled
late DOM delivery, not the timing or frequency of delayed original asset-protocol decoding.
Pan behavior is covered in the component regression.

The first desktop run failed because its assertion expected `file_name` on legacy
`list_assets` details, which expose `path`. The final spec compares the path basename;
both scenarios then passed. This was a fixture assertion correction, with no production
change prompted by the failed run. Both isolated profiles were removed after their sessions
exited. The cleanup record confirms no remaining Tauri driver, WebKit driver, Xvfb or app
processes. The spec removed its synthetic media roots.

Logs, batch-start source snapshots, the desktop launch script, cleanup result and read-only
historical-hash comparison are in [batch evidence](../../../raports/20261012-batch-07/).
Native screenshots and JSON observations are in
[desktop evidence](../../../artifacts/lightbox-recovery/). The historical review files and
hashes remain unchanged. Their recorded source drift is expected after repairs; the
historical validator was not rerun or rewritten.

Verification covers Linux X11/WebKit in the isolated debug profile, mocked-IPC frontend
race tests and the actual native page rejection/recovery path. Wayland, release packaging,
GIF decode timing and native video controls were not checked. No Rust or IPC shape changed
in this batch, so standalone backend tests were not rerun. Combined full-suite validation
remains batch 14. Next is batch 08, DR-008.


## Batch 08 completed, 2026-10-12

Worked in the existing checkout at `be27032bacb5763e79af171ae4fecb64e0c86708` with
batches 01 through 07 still uncommitted. Earlier changes remain intact. No worktree,
commit or push was created.

DR-008 is fixed in [createGridHandlers.ts](../../../src/features/gallery/createGridHandlers.ts).
Geometry changes cancel a held pointer gesture and remove obsolete preview geometry.
Released global-index reads and settled failed Retry intent survive listener re-registration,
window resizing and observer measurements. Separate memoized mode/query dependencies retain
explicit cancellation when those values change, without treating geometry-only prop updates
as a new selection context. New input, blur, pointer cancellation and disposal still cancel;
the selection owner continues to invalidate work on identity reset.

[GalleryGrid regressions](../../../src/features/gallery/__tests__/GalleryGrid.test.tsx)
compose the real gallery, virtualizer, selection owner and query cache with mocked IPC.
They evict page zero, fail its offscreen range read, render the production error banner,
and deliver the measurement observer after its 48px simulated geometry change. Retry
completes the original Shift/additive-rectangle/replacement-rectangle ranges. Previous IDs
survive failure; successful replacement replaces them, while additive retries retain the
ID outside the range. [Handler tests](../../../src/features/gallery/__tests__/createGridHandlers.test.tsx)
cover pending released reads across layout changes and explicit cancellation of failed retries.
Held gestures still cancel on layout, window resize, query/mode changes and disposal.
[Selection tests](../../../src/features/gallery/__tests__/createSelection.test.ts) also check
pending and settled failure invalidation on identity reset.

Verification from the repository root:

```sh
# Before the fix: 2 new cases failed at erased selection.error; 3 existing cases passed.
bun run test -- src/features/gallery/__tests__/GalleryGrid.test.tsx

# Final focused run: 9 files, 104 tests passed.
bun run test -- src/features/gallery src/library/query/__tests__/createAssetQuery.test.ts src/app/__tests__/gallery.test.tsx

# Discovery returned only GalleryGrid.test.tsx. The probe substitutes the saved original
# handler during Vite transformation, without changing on-disk production source.
node node_modules/vitest/vitest.mjs list --config raports/20261012-batch-08/probe.config.ts --filesOnly --json
# Expected exit 1: all 3 final recovery cases failed, 3 existing controls passed.
node node_modules/vitest/vitest.mjs run --config raports/20261012-batch-08/probe.config.ts --reporter=json --outputFile=raports/20261012-batch-08/original-results.json

# Both passed.
bun run typecheck
bun run lint

# Private Xvfb/D-Bus session, disposable XDG profile, opt-in selection recovery.
# Fresh frontend and E2E Tauri builds, then 2/2 native scenarios passed.
sh raports/20261012-batch-08/run-desktop.sh

git diff --check
```

The new [desktop spec](../../../e2e/specs/selection-recovery.e2e.js) indexes 2,052 copied PNGs,
traverses more than twelve pages and rejects evicted page 128 through an invalid unsigned
argument sent to the real native command. Native Ctrl+Shift-click and Ctrl-drag both leave
exactly two existing IDs after failure. Actual ResizeObserver delivery records the grid
margin changing from 20px to 74px when the banner appears. Clicking visible Retry requests
page 128 exactly once after recovery, completes indices 0 through 1795, and retains the
previous ID at index 2051, for 1,797 selected IDs. Banner removal restores the 20px margin.
Both endpoint checks and all four saved failure/recovery screenshots were inspected.

Initial desktop attempts exposed test-input problems. Scrolling now waits for the target's
measured viewport position after bulk-panel layout. Explicit key actions hold and release
Ctrl/Shift across pointer input; `browser.keys` released them before the click, while relying
only on `releaseActions` left Shift affecting the following scenario on this driver. Count
checks now compare the parsed integer exactly. Final runs contain no failed assertions.
The logs retain host graphics/keyboard messages and a D-Bus message during teardown; this
is not a claim of warning-free execution.

The gallery, bulk-selection and testing guides describe the corrected contract and opt-in
suite. Logs, the original-source probe, historical hash comparison and cleanup record are in
[`raports/20261012-batch-08/`](../../../raports/20261012-batch-08/). Desktop screenshots,
page requests and geometry samples remain under
[`artifacts/selection-recovery/`](../../../artifacts/selection-recovery/). All four temporary
profiles and synthetic media roots are gone; no test native processes or profile mounts
remain. Historical review evidence and hashes were not rewritten. The read-only comparison
records intentional source/documentation drift from this and earlier batches.

Verification covers Linux X11/WebKitGTK and controlled frontend failures. Wayland, release
packaging, full frontend/backend suites and the combined batch-14 checks did not run.
No backend, IPC or dependency changes were needed. Next is batch 09, DR-022.


## Batch 09 completed, 2026-10-17

Worked in the existing checkout at `be27032bacb5763e79af171ae4fecb64e0c86708`,
with batches 01 through 08 still uncommitted. Their changes remain intact. No
worktree, commit or push was created.

DR-022 is fixed. Changes are confined to the [gallery performance spec](../../../e2e/specs/gallery.performance.e2e.js)
and its documentation. The timing case now precedes metadata traversal. It samples
16 image-only stops, indices 128 through 1920 in steps of 128, followed by 2047,
and revisits exactly those stops with the same filter. The initial mounted area
and the starting target at zero are excluded; final returns to 128/0 are untimed.
This does not exclude every row in query page zero: target 128 can expose indices
126/127, whose preview files are still checked independently.

Preparation clears the isolated library and thumbnail files before indexing.
Each sample inventories final JPEG filenames before scrolling and records the
visible IDs, indices and filenames after decoding. Filesystem inventory includes
published files even before their SQLite reference appears. Samples are classified
as first-use, mixed or existing previews from that evidence. Only samples with no
existing visible preview enter the first-use summary; revisits must find every
visible preview already on disk and must show the same IDs/targets as the first
pass. The requested target must be visible. Raw elapsed times and frame intervals
retain their order; percentile calculation sorts a copy. File inventory and JSON
writes are outside the measured intervals, and the frame sampler stops in `finally`.

The final control passed all four cases in declared order: timing, stable metadata,
static GIF/animated lightbox, and bulk selection. Before sampling, 56 final JPEGs
existed, 56 tiles were mounted and 28 were visible. All 16 first-traversal samples
had no previously saved visible previews; all 16 revisits reused their preview
files. At a 1440×900 WebView viewport, observed thumbnail p95 was 852.30 ms for
first-use samples and 358.95 ms for revisits; frame-interval p95 was 19 ms and
43 ms, from 550 and 152 raw intervals respectively. These include WebDriver and
polling overhead, exclude the opacity fade, and are not evidence of a production
slowdown or a portable speed comparison. The gallery screenshot and two native
GIF captures were inspected; the GIF's decoded native pixels change between frames.

An earlier complete control passed 4/4. A subsequent run passed timing but timed
out in the pre-existing metadata traversal after closing the preview; the GIF
case then inherited that unfinished case's filter. The modal layer restores focus
on a later animation frame, allowing that restoration to race the next scroll.
The spec now waits for both dialog removal and actual focus on the triggering tile
at its two metadata traversal closes. The final complete control passed with that
barrier. No production focus or gallery behavior changed.

Verification commands from the repository root:

```sh
# Fresh frontend and native E2E builds, private Xvfb/D-Bus, disposable XDG profile.
# Final control: exit 0, all four cases passed.
sh raports/20261017-batch-09/run-desktop.sh

# Generate an ignored spec copy with metadata registered before timing.
# Imports and output paths are adjusted for the copy; source is not modified.
python3 raports/20261017-batch-09/make-order-probe.py
# Expected exit 1: the first-use assertion fails; three remaining cases pass.
sh raports/20261017-batch-09/run-desktop.sh raports/20261017-batch-09/reordered.e2e.js

node --check e2e/specs/gallery.performance.e2e.js
bun run lint
python3 raports/20261017-batch-09/verify.py
python3 raports/20261017-batch-09/cleanup.py
git diff --check
```

Evidence and the decision trail are in
[`raports/20261017-batch-09/`](../../../raports/20261017-batch-09/).
The final control is `desktop-final.log`; the failed intermediate control is
`desktop-focus-failure.log`. Current raw samples and screenshots are in
[`artifacts/gallery-performance/`](../../../artifacts/gallery-performance/).
The reordered probe has its own output directory. Historical review hashes and
reports remain unchanged; the verifier records current hashes beside the historical
values in `historical-drift.json` without rewriting them.

Scope is Linux X11/WebKitGTK in the unbundled debug E2E profile with synthetic
fixtures. The OS cache was not flushed; absent preview files do not establish
uncached source reads or exclude queued rendering. Revisits also cross the
frontend page-cache bound. Wayland, release packaging, million-result scrolling,
standalone frontend/backend suites and combined batch-14 validation were not run.
The E2E build includes TypeScript checks and the production frontend build. No
backend, IPC, application-source or dependency changes were needed.

The final reordered probe failed only the first-use assertion because metadata
traversal had already generated the sampled previews; its other three cases passed.
`verify.py` passed after independently recomputing each sample's provenance and
summary percentiles, checking identical visible IDs/targets, excluded initial IDs,
full-suite execution order and the expected single probe failure. It also confirmed
that the source hash did not change during the probe. Lint, JavaScript syntax and
`git diff --check` passed. All local documentation links and anchors in the five
changed Markdown files resolved.

Cleanup verified removal of all five temporary profiles and five synthetic media
roots from the control/probe attempts, with no owned processes or profile mounts
remaining. `cleanup.json` retains that result. Logs include host graphics messages,
the expected selection-IPC rejection and a D-Bus teardown message; these checks do
not establish warning-free execution. The decision trail received independent
review; its wording correction distinguished target zero from the entire first
query page. Batch 09 stops here. Next is batch 10, DR-007 and DR-009.


## Batch 10 completed, 2026-10-17

Worked in the current checkout at `be27032bacb5763e79af171ae4fecb64e0c86708`,
with batches 01 through 09 still uncommitted. Their edits remain intact. No worktree,
commit or push was created.

DR-007 is fixed in [scroll coordinates](../../../src/features/gallery/scrollCoordinates.ts),
[the virtualizer adapter](../../../src/features/gallery/createVirtualGrid.ts),
[gallery placement](../../../src/features/gallery/GalleryGrid.tsx) and
[pointer handling](../../../src/features/gallery/createGridHandlers.ts).
The physical spacer stops at 4,000,000 pixels. Virtual-core keeps logical row
measurements, while the offset observer and tile/backplate positions translate
between the physical scrollbar and logical content. Header margin and footer tail
remain in ordinary pixels. Wheel and rectangle edge scrolling use logical deltas;
rounded `scrollTop` writes retain their intended subpixel offset. Unchanged row
ranges retain identity so pixel movement does not republish page demand.

Resize preserves the first visible global index. The native viewport-height case
exposed a delayed virtual-core scroll-end callback carrying the pre-resize offset.
A fake-timer regression failed before the correction. The adapter now reads the
current DOM offset when that callback runs, preserving the newer anchor. Grid
height is applied before restoring the anchor. Pointer rectangles use logical
coordinates for selection and physical coordinates for their visible overlay;
scroll translation alone does not cancel a held gesture.

The new [native fixture](../../../e2e/gallery-scale/fixture.tsx) executes the
real gallery, selection model, virtual-core and CSS in standalone WebKitGTK 2.54.1.
Its final run passed ten recorded checks. One million results reached visible index
999999 and selected ID 1000000 by native click at five columns, a narrow single
column, a large-tile single column, six columns and eight columns. Those endpoints
mounted 35, 6, 5, 46 and 56 tiles respectively. A 100-result control remained
uncompressed. Width, tile-size and viewport-height changes retained the anchor.
XTest dragged the actual scrollbar thumb to the last row, delivered wheel input,
and selected global IDs with Shift-click and a rectangle around index 500,000.
The fixture also checked range publication and DOM removal after disposal.
The final-row and selected-tile screenshots were opened and inspected.

An ignored Vite loader substituted only the original `GalleryGrid.tsx` and
`createVirtualGrid.ts` in memory for the same native fixture. Their `HEAD` hashes
match the historical DR-007 hashes. The expected failure reproduced a physical
height of 33,554,428 pixels and a demanded range ending at 838884, with the final
result absent. Application source on disk was not reverted for this probe.
Historical review evidence remains unchanged. `historical-drift.json` records
current hashes beside the original hashes; it does not replace them.

DR-009 is fixed in the [gallery guide](../../subsystems/library-query-and-gallery.md).
`AppGalleryView` passes `query.offset < query.total`, so the two-row near-end loader
remains active alongside primary sparse viewport demand. `offset` is the highest
merged page end, not proof that every earlier page is cached. `GalleryGrid` passes
the required query total as the footer's required `resultCount`, with no array-length
fallback. Generation plus `hasMore` shows progress; idle generation plus no more
rows shows the end state; the other two combinations render no footer content.
The required external scroller accessor is also documented accurately.

The integrated gallery regression uses the real sparse cache with a mocked transport,
requests final-page offset 999936, selects the last loaded tile and shows 1,000,000
in the footer while retaining at most twelve pages. Another caller regression checks
thumbnail progress disappearing when the highest page reaches the total. The footer
unit cases cover all four generation/`hasMore` branches. Existing selection-retry,
cache eviction, measurement reuse, null-ref startup and cleanup checks still pass.

Verification from the repository root:

```sh
# 11 files, 132 tests passed.
bun run test -- src/features/gallery src/app/__tests__/gallery.test.tsx src/library/__tests__/createLibrary.test.ts src/library/query/__tests__/createAssetQuery.test.ts
bun run typecheck
bunx tsc --noEmit -p e2e/gallery-scale/tsconfig.json
bun run lint
# 194 production modules; Solid entry and dependency checks passed.
bun run test:build
# Final native control, exit 0.
sh e2e/gallery-scale/run.sh
# Original-source native probe, expected exit 1 at last-result visibility.
sh raports/20261017-batch-10/run-baseline.sh
# Run after rebuilding/running the corrected native fixture.
python3 raports/20261017-batch-10/verify.py
git diff --check
```

Evidence and the decision trail remain in
[`raports/20261017-batch-10/`](../../../raports/20261017-batch-10/).
The final control log is `native-final-3.log`; native results, screenshots and cleanup
records are under ignored [`artifacts/gallery-scale/`](../../../artifacts/gallery-scale/).
The first native attempt could not open Xvfb within the sandbox. The authorized
private-display rerun exposed missing Tailwind utilities and an overflowing test
`scrollTop` assignment. The fixture now scans production sources explicitly and
scrolls to the actual `scrollHeight`. Later failures isolated the viewport-height
callback race described above. Their logs remain beside the successful control.
Native logs contain host DRI3 warnings and ResizeObserver notification warnings;
these results do not establish warning-free execution.

The runner removed its disposable XDG profiles, closed the local server/thread and
exited its private display. The evidence verifier independently checked profile and
owned-process absence, the old/new native outcomes, test/build results, historical
hash correspondence and the five updated documentation files' links and anchors.

Scope is Linux X11/WebKitGTK layout and native pointer input with lazy synthetic
summaries and synthetic range reads. Page callbacks establish demand, not live IPC.
No million-file SQLite query, real thumbnail rendering, Wayland, release packaging,
keyboard/touch navigation or performance comparison was tested in this batch.
The existing batch-09 measurements remain the performance baseline. Backend and
IPC code did not change; Rust checks and the full combined suite remain for batch 14.
A cross-model trail audit by `gpt-6-astra` corrected one log pointer by an appended
row. The initial failing fake-timer output had not been saved, so a separate
post-audit Vite transform restored only the stale-offset callback in memory.
Discovery found exactly `createVirtualGrid.test.tsx`; the targeted regression failed
with one failure and eight skipped tests, while the corrected 132-test control
remains green. This saved reproduction establishes the regression's sensitivity;
it does not reconstruct the unsaved run's chronology. Evidence is in
`scroll-end-mutation.log`, `scroll-end-discovery.json` and `trail-review.md`.
The probe is reproducible with:

```sh
node node_modules/vitest/vitest.mjs run --config raports/20261017-batch-10/scroll-end-probe.config.ts -t 'ignores a delayed scroll-end'
```

Next is batch 11, DR-016.


## Batch 11 verification, 2026-10-17

DR-016 is fixed. Work continued in the existing checkout with batches 01 through 10
already modified and uncommitted. Those changes remain intact. No worktree, commit
or push was created.

The GTK capture handler in [`video_surface.rs`](../../../src-tauri/src/video_surface.rs)
now sends an `escape` intent only when windowed native controls have focus. Menu
Escape and fullscreen exit still run first. WebView-focus input remains on the DOM
path. [`video_player_service.rs`](../../../src-tauri/src/services/video_player_service.rs)
uses the existing session channel and rejects retired session IDs. The matching
[wire type](../../../src/platform/tauri/contracts/video.ts),
[session model](../../../src/features/lightbox/video/createNativeVideoSession.ts),
[player](../../../src/features/lightbox/video/LightboxVideoPlayer.tsx) and
[media stage](../../../src/features/lightbox/LightboxMediaStage.tsx) carry the callback
to [`LightboxContent`](../../../src/features/lightbox/LightboxModal.tsx).
That owner checks active/top-layer state, fullscreen and synchronous deletion
ownership. It dismisses confirmation before the narrow drawer or preview. An input
from a retired session cannot close a replacement preview.

The two initial component regressions failed against unchanged production code,
with 11 existing cases passing. Their saved failure shows the confirmation and
current selection retained after Escape. The corrected focused run passed 67 tests
across six files. These include current/retired and early/disposed input, higher
registered layers, pending deletion, confirmation/drawer priority, existing DOM
keyboard handling and fullscreen continuity. The adapter test delivers the exact
`{ session_id, type: "escape" }` payload. A Rust channel test checks serialization,
current-session delivery and rejection after retirement. Its first version called
`close` on a deliberately workerless service and failed on worker availability;
the corrected test uses the service's actual retirement operation.

The new [`native-video-escape.e2e.js`](../../../e2e/specs/native-video-escape.e2e.js)
passed all seven cases on a private Xvfb display with temporary MP4/GIF fixtures and
an isolated E2E profile. Play focus was established by Space changing pause state;
volume focus by End/Home changing volume to one/zero. Neither input reached DOM
listeners. Physical Escape then removed the native player in both cases. The
WebView control received one DOM Escape and closed normally. Other cases preserved
the session when dismissing the speed menu, exited fullscreen on the next Escape,
dismissed inline delete confirmation before closing, and closed the narrow drawer
before the preview. Screenshots of focused controls, the resulting gallery,
dismissed confirmation and collapsed drawer were opened and inspected.

Verification from the repository root:

```sh
# 6 files, 67 tests passed.
bun run test -- src/features/lightbox/video src/features/lightbox/__tests__/LightboxModal.test.tsx src/features/lightbox/__tests__/keyboard.test.tsx src/platform/tauri/__tests__/api.test.ts
# 22 passed, 171 filtered out.
cargo test --manifest-path src-tauri/Cargo.toml --locked --offline --lib video
bun run typecheck
bun run lint
bun run rust:fmt
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --all-features --locked --offline -- -D warnings
node --check e2e/specs/native-video-escape.e2e.js
# 7 desktop cases passed; includes fresh frontend and Tauri E2E builds.
sh raports/20261017-batch-11/run-desktop.sh
git diff --check
```

Logs remain in [`raports/20261017-batch-11/`](../../../raports/20261017-batch-11/).
`frontend-baseline.log` contains the expected failures; `frontend-final.log`,
`rust-video-final.log`, `typecheck-final.log`, `lint-final.log`, `rust-fmt.log` and
`clippy.log` contain the passing checks. `native-final.log` records seven passing
cases and WDIO exit zero. The outer launcher exited one because temporary D-Bus
mounts were still releasing during its cleanup trap. Subsequent inspection found
no owned processes or mounts, and both run profiles were removed; `cleanup.json`
records their absence. The final native artifacts are under
[`artifacts/native-video-escape/2026-10-17T08-19-27-830Z/`](../../../artifacts/native-video-escape/2026-10-17T08-19-27-830Z/).

The first desktop launch was blocked by the sandbox's local-socket restriction.
An approved private-display rerun passed the five core focus/menu/fullscreen cases,
but two new fixture cases tried to click a faded control or measured the drawer
during its transition. The test now moves the native pointer to reveal controls,
waits for hit-testable geometry, and measures again before clicking. No application
change was needed for those test failures. Logs retain the failed run and host
D-Bus/accessibility warnings; this was not warning-free execution.

Historical review evidence remains unchanged. `historical-drift.json` records
current hashes beside the original DR-016 hashes without replacing them. The owning
lightbox, frontend layer, IPC and testing guides describe the new behavior.

Scope is Linux X11/WebKitGTK with synthetic XTest input. Retired input, higher
registered layers and pending deletion use controlled frontend/Rust checks.
Wayland, hardware keyboards, release packaging and full playback/performance
coverage were not tested. Combined project verification remains batch 14.

Next is batch 12, DR-018.


## Batch 12 verification, 2026-10-17

DR-018 is fixed. Work used the existing checkout with batches 01 through 11 already
modified and uncommitted. Existing changes remain intact. No worktree, commit or push
was created.

[`createScanSettings.ts`](../../../src/features/settings/createScanSettings.ts)
assigns a synchronous token inside each admitted thumbnail render action. Stop captures
that token and publishes accepted, false or rejected replies only while it still owns
the scan section. The render's `finally` retires ownership and clears its pending Stop
state before the runner releases the lock. A new render can send Stop while an old
reply remains outstanding; the old request's `finally` cannot clear the new request's
pending state. Stop without an active render is ignored. Both Render all and Retry
failed retain ordinary active-render feedback, duplicate admission guards and retry.

The new [`thumbnailStop.test.tsx`](../../../src/features/settings/__tests__/thumbnailStop.test.tsx)
contains 38 controlled-response cases. Both render modes cross success/failure with
accepted/false/rejected Stop replies, preserving either the terminal result or the next
scan's progress. Additional cases cover active Stop feedback, repeat admission, a newer
render's pending Stop, listener registration, disposal and a rendered interaction.
That interaction attaches a synthetic root, clicks Render and Stop, observes the disabled
Stop button, completes the render with failure, clicks the enabled Rescan all button and
checks that a late Stop preserves the status text and progress-bar value.

The existing cancellation-error test in
[`createSettingsActions.test.tsx`](../../../src/features/settings/__tests__/createSettingsActions.test.tsx)
now starts a render before calling Stop, matching the button's availability. Its first
broader run failed because it previously called Stop from idle. The first new component
test also queried `role="status"` after loading ended; it now checks the terminal error
text and uses the status role during scanning. These test corrections are included in
the final original-source comparison.

Verification from the repository root:

```sh
# 20 files, 135 tests passed, including the 38 new cases.
bun run test -- src/features/settings
bun run typecheck
bun run lint
# Fresh production build, 194 modules; Solid/dependency checks passed.
bun run test:build
git diff --check

# Discovery returned only thumbnailStop.test.tsx.
node node_modules/vitest/vitest.mjs list --config raports/20261017-batch-12/original-probe.config.ts --filesOnly --json
# Expected exit 1 with pre-fix controller loaded in memory: 29 failed, 9 passed.
node node_modules/vitest/vitest.mjs run --config raports/20261017-batch-12/original-probe.config.ts
```

The probe uses the controller snapshot taken before this batch, preserving prior repairs.
It does not replace application source on disk. The failures establish that the tests
detect stale terminal/progress publication, blocked Stop admission on a new render and
idle Stop admission. Nine active-render/disposal controls still pass. The initial
baseline also reproduced terminal overwrites before the implementation changed.

Logs and the source snapshot remain under
[`raports/20261017-batch-12/`](../../../raports/20261017-batch-12/).
`settings-final.log`, `typecheck-final.log`, `lint-final.log` and `build.log` contain the
passing checks; `original-source-final.log` records the expected regression failures.
`historical-drift.json` compares all 43 DR-018 review hashes with this checkout, with
11 differences. Historical review evidence and hashes remain unchanged. The settings,
thumbnail and progress IPC guides now describe local render ownership and late replies.

Verification covers production Solid models, the operation runner and rendered controls
with controlled adapter replies. No native rendering, live Tauri delivery frequency,
Wayland or release packaging was tested in this batch. Backend cancellation and wire
payloads are unchanged. Combined project verification remains batch 14.

Next is batch 13, DR-019.

## Batch 13 verification, 2026-10-17

DR-019 is fixed. The nine resources under [`src/i18n/locales`](../../../src/i18n/locales/)
for `cs`, `de`, `es`, `fr`, `it`, `ja`, `ko`, `ru` and `zh` each change thirteen tag-list
values and four search errors, 153 values total. Placeholders and grammar identifiers
remain intact. English, Polish, and the shared `{{tag}} ({{state}})` title template are
unchanged. Existing uncommitted repairs remain intact; no worktree, commit or push was made.

[`localization.test.tsx`](../../../src/features/search/__tests__/localization.test.tsx)
uses the real i18next resources, Solid provider, tag modal, search controller, parser and
search wrapper. Mocked tag discovery returns three synthetic names. In each of eleven
locales, clicks and Shift+Enter produce one included, one excluded and one inactive tag,
with the rendered summary, accessible labels and titles recorded in reviewed snapshots.
Apply still emits `alpha -beta`. Four invalid inputs produce 44 rendered alerts across
the eleven locales. The five loading/empty/applying messages are pinned from resources,
not all exercised as rendered branches. English and Polish are controls.

The original-resource probe replaces JSON only during Vite transformation. Discovery
returned only the new localization test. Against the saved pre-batch resources, nine
snapshots fail on the copied English values and both control locales pass. On-disk
resources and historical review artifacts are not changed by the probe. No blanket
English-equality rule was added.

Verification from the repository root:

```sh
# 15 files, 112 tests passed, including 11 locale cases.
bun run test -- src/features/search src/app/__tests__/shell.test.tsx src/features/lightbox/__tests__/LightboxModal.test.tsx
# Final focused rerun without snapshot updates: 11 passed.
bun run test -- src/features/search/__tests__/localization.test.tsx
bun run locale:check
bun run test:locale-tools
bun run typecheck
bun run lint
bun run test:build
git diff --check

node node_modules/vitest/vitest.mjs list --config raports/20261017-batch-13/original-probe.config.ts --filesOnly --json
# Expected exit 1: nine failed, English and Polish passed.
node node_modules/vitest/vitest.mjs run --config raports/20261017-batch-13/original-probe.config.ts
python3 raports/20261017-batch-13/audit.py
```

All ordinary checks exited 0. Locale parity passed for eleven languages, the locale-tool
suite passed, lint checked 266 files without fixes, and the production build passed its
194-module Solid/dependency check. The audit proves exactly 153 scoped changes and records
17 differences among 43 historical DR-019 hashes. Historical hashes remain immutable.
Logs, original resources, probe configuration and audit output remain in ignored
[`raports/20261017-batch-13/`](../../../raports/20261017-batch-13/).

The persistent desktop controller used a fresh E2E build, private Xvfb display and disposable
seven-media fixture profile. Visible interactions and inspected screenshots confirmed the
English and German tag-list filter, zero-count summary and empty message at 1440×900.
Selecting Deutsch through Settings updated the UI, and submitting `tags cat` rendered the
German metatag error without clipping. The initial sandbox launch could not open local
sockets; the authorized isolated launch succeeded outside the sandbox. Clicking a hidden
select option timed out, so the visible selector was scrolled into view and used by keyboard.
No production profile or personal media was accessed.

Desktop evidence is under
[`artifacts/app-control/eac3ecbe1562f60af95951b33a32e728/`](../../../artifacts/app-control/eac3ecbe1562f60af95951b33a32e728/).
Screenshots 0001–0003 were inspected. The console recorded one ResizeObserver notification
loop warning during Settings interaction; no other captured application error appeared.
Startup messages before capture attachment are unavailable. Stop was called twice;
the manifest reports `stopped`, the owned temporary root is removed, and evidence remains.

The localization and search/tag guides now describe the translated output and regression
coverage. Linguistic review was by the implementing agent, without independent native-speaker
review. DOM accessible labels do not prove speech output. Other locale layouts, screen readers,
Wayland and release packaging remain unverified. No backend behavior changed, and backend
or combined project suites were not run in this batch. Next is batch 14.

## Batch 14 combined verification, 2026-10-17

Work uses the existing checkout at `be27032bacb5763e79af171ae4fecb64e0c86708`, with
batches 01 through 13 uncommitted. Their source and documentation changes remain intact.
No worktree, commit, push, dependency upgrade or application redesign was created.

The nine required combined commands all passed on this repair state:

| Command | Result |
| --- | --- |
| `bun run typecheck` | Both TypeScript projects passed. |
| `bun run lint` | Passed, no fixes applied. |
| `bun run test` | 80 files, 761 tests passed. |
| `bun run test:build` | Fresh production build and 194-module Solid/dependency check passed. |
| `bun run locale:check` | All eleven locale resources passed. |
| `bun run test:locale-tools` | Node test runner passed the locale-tool regression file. |
| `bun run rust:fmt` | Passed. |
| `bun run rust:clippy` | All targets/features passed with warnings denied. |
| `bun run test:backend` | 193 library tests and 19 integration/workflow tests passed; two opt-in diagnostics ignored. |

The Rust integration counts come from seven discovered binaries, including the binary
whose only diagnostic is ignored. The two ignored tests are `metadata_scale_measurements`
and `tag_save_benchmark`. They are not required repair acceptance checks. Counts above
are from one combined run per layer, not sums of overlapping focused runs.

Exact commands, exit codes, durations and full logs are in
[`raports/20261017-batch-14/`](../../../raports/20261017-batch-14/).
The local `checks.py` runner serializes the nine commands; `checks.json` records their
results. `desktop.py` serializes the affected desktop specs with all six required opt-in
flags, a fresh E2E build, private Xvfb/D-Bus and disposable XDG directories.

The first desktop invocation completed eight specs: six passed and two failed during
fixture setup. Its log remains `desktop.log`; it is not a passing combined invocation.
The rectangle case timed out before failure injection. A diagnostic rerun recorded forty
samples where tile 128 existed and its top was 0.019 to 0.944 pixels above the viewport
boundary. `selection-recovery.e2e.js` now allows one pixel of WebKit scroll rounding and
records the last forty attempts on failure. The original setup still fails with diagnostics
alone, recorded in `input-check/selection-failure.json`.

The narrow video drawer case failed before its Escape assertion, and its cleanup also
could not click the fading Close control. The failure screenshot shows hidden controls;
it does not by itself establish why the click failed. The spec now uses its existing native
pointer/reveal helper for the drawer trigger and cleanup Close. All seven native Escape
cases then passed in `input-check/desktop.log`, including the drawer and cleanup. That
invocation still exits one because it also contains the diagnostic selection failure.
No production code changed in this batch. The testing guide documents both fixture changes;
final lint, JavaScript syntax checks and tracked/untracked whitespace checks passed.

Both final selection cases passed in `selection-final/desktop.log`, exit zero. Each
retained the two prior IDs through failure, recovered 1,797 selected IDs through Retry,
and preserved the endpoint outside the range. The latest result for every affected
desktop case is passing:

| Desktop spec | Cases passed | Passing evidence |
| --- | --- | --- |
| Startup scan | 1 | `desktop.log` |
| Startup remount | 2 | `desktop.log` |
| Search/tag recovery | 12 | `desktop.log` |
| Lightbox recovery/image ownership | 2 | `desktop.log` |
| Selection recovery | 2 | `selection-final/desktop.log` |
| Gallery performance, full declared order | 4 | `desktop.log` |
| Native video Escape | 7 | `input-check/desktop.log` |
| Existing application workflows | 13 | `desktop.log` |

These are 43 distinct passing cases across the initial run and targeted reruns, not a
single all-green desktop invocation and not a sum including duplicate executions.
`verify-final.py` checks the latest status for each case from the raw logs.

The performance evidence contains sixteen first-use samples and sixteen revisits over
the same image route. Independent file-inventory checks confirmed no existing visible
previews in each first-use sample and all visible previews present in each revisit.
Timing remains observational, includes WebDriver overhead and does not establish an
OS-cold cache or a portable performance comparison.

`bunx tsc --noEmit -p e2e/gallery-scale/tsconfig.json` and
`sh e2e/gallery-scale/run.sh` passed. The latter ran ten checks in WebKitGTK 2.54.1:
last-result access across five layouts, an uncompressed control, resize/tile anchoring,
physical scrollbar dragging, wheel input and native Shift/rectangle selection. Every
million-result layout reached index 999999 with a 4,000,000px spacer and fewer than
120 mounted tiles. This fixture uses lazy synthetic summaries and real production
gallery/selection code; it does not establish million-row SQLite or thumbnail scale.

Inspected screenshots show the recovered selection count, recovered suggestion, current
lightbox image, gallery after native Escape, dismissed drawer and million-result footer.
The scale fixture intentionally uses placeholders. Screenshot inspection does not expand
the recorded playback or accessibility scope. Copies of this run's screenshots and JSON
observations are preserved in `initial-artifacts/`, `input-check-artifacts/` and
`final-artifacts/` under the batch evidence directory. Stale failure artifacts in shared
`artifacts/` are not treated as current passing evidence.

Desktop sessions and builds were serialized. The isolated launch required permission for
local WebDriver sockets. A rerun port preflight initially rejected a released port in
TIME_WAIT; enabling socket address reuse allowed the check without killing any listener.
All three temporary E2E profiles were removed after verifying no owning processes or
mounts. `cleanup.json` supersedes the earlier `removed: false` records retained after
failed runs. The scale profile and server were removed too; `native-processes.json`
records no remaining app, driver or Xvfb processes. Logs retain host Mesa/accessibility/
D-Bus warnings, WebDriver stale-element retries, expected rejected IPC validation, and
ResizeObserver notification-loop warnings in the scale fixture. This is not warning-free
execution.

`audit.py` compared 693 historical hash entries across 224 paths and recorded 258 changed
entries. These are expected source/documentation differences after the repairs. All
1,375 historical review artifact hashes stayed unchanged during this batch; no original
verifier hashes were replaced. `doc-check.py` checks local links/anchors and both tracked
and untracked whitespace. `verify-final.py` checks combined command results, desktop
case dispositions, scale/provenance assertions and cleanup.

Each of the twenty confirmed findings is fixed. The individual batch records above retain
its broken-source or mutation comparison, final regression, owning documentation and
limits. The combined suites reran those regression files on the final shared state.
DR-017 and DR-021 remain rejected and are not counted as repairs.

| Finding | Changed owner and regression checked in the combined run | Evidence and limits |
| --- | --- | --- |
| DR-001 | [Testing guide](../../development/testing.md), package scripts and Cargo-discovered test targets | [Batch 01](#batch-01-verification-2026-10-11); the full backend run confirms target discovery and ignored diagnostics. |
| DR-002 | [runtime.test.ts](../../../src/app/__tests__/runtime.test.ts) | [Batch 01](#batch-01-verification-2026-10-11); completion-aware disposal test passed. Original mutation evidence retained. |
| DR-003 | [Startup state](../../../src-tauri/src/app/state.rs), [scan command](../../../src-tauri/src/commands/scan.rs), [scan model](../../../src/features/settings/createScanSettings.ts), startupScanRemount tests | [Batch 03](#batch-03-verification-2026-10-11); native retained completion and desktop reload during blocked probing passed. Failure ordering remains controlled-test evidence. |
| DR-004 | [Query service](../../../src-tauri/src/services/asset_query_service.rs), [command](../../../src-tauri/src/commands/assets.rs), [adapter](../../../src/platform/tauri/assets.ts), [query model](../../../src/library/query/createAssetQuery.ts) | [Batch 04](#batch-04-verification-2026-10-12); inverted admission/reply and owner-remount tests passed; live query workflows passed. Inversion is injected in tests. |
| DR-005 | Same query service/model; native capacity tests and rendered App gallery errors | [Batch 04](#batch-04-verification-2026-10-12); tiny-budget rejection and narrowing recovery passed. Desktop does not force a production-budget overflow. |
| DR-006 | [Details cache](../../../src/library/metadata/createDetailsCache.ts), [mutations](../../../src/library/metadata/createMutations.ts), runtimeIntegration tests | [Batch 05](#batch-05-completed-2026-10-12); deferred Settings rename/rescan and save publication passed with mocked transport. |
| DR-007 | [GalleryGrid](../../../src/features/gallery/GalleryGrid.tsx), [virtual grid](../../../src/features/gallery/createVirtualGrid.ts), [coordinates](../../../src/features/gallery/scrollCoordinates.ts), gallery tests | [Batch 10](#batch-10-completed-2026-10-17); native million-result fixture and real sparse-cache component tests. Scale fixture uses lazy synthetic summaries, not million-row SQLite. |
| DR-008 | [Grid handlers](../../../src/features/gallery/createGridHandlers.ts), GalleryGrid and handler regressions | [Batch 08](#batch-08-completed-2026-10-12); component banner geometry and native Shift/rectangle recovery passed. |
| DR-009 | [Gallery guide](../../subsystems/library-query-and-gallery.md), GalleryGrid and GalleryStatusFooter tests | [Batch 10](#batch-10-completed-2026-10-17); required total, offset/total caller and all footer branches checked. |
| DR-010 | [Search wrapper](../../../src/features/search/SearchTagator.tsx), [suggestions](../../../src/features/search/suggestions/createSuggestions.ts), suggestionRecovery tests | [Batch 06](#batch-06-completed-2026-10-12); native pointer and keyboard recovery passed in search, lightbox and bulk editors. Worker failure is injected. |
| DR-011 | [Tag selection](../../../src/features/search/tag-list/tagListSelectionService.ts), [modal](../../../src/features/search/tag-list/TagListModal.tsx), TagListModal and literalTagSearch tests | [Batch 06](#batch-06-completed-2026-10-12); both literal names passed click, double-click and keyboard desktop checks. |
| DR-012 | [Lightbox model](../../../src/features/lightbox/createLightbox.ts), query retry owner, createLightbox tests | [Batch 07](#batch-07-verification-2026-10-12); native page-128 rejection, visible Retry and cached Previous passed. Rejection is injected through the actual IPC boundary. |
| DR-013 | Mutation/lightbox owners; mutations and createLightbox tests | [Batch 05](#batch-05-completed-2026-10-12); saved favorite and failed refresh remain separate. Failure ordering uses deferred adapters. |
| DR-014 | [Image controls](../../../src/features/lightbox/image/createImageControls.ts), [media stage](../../../src/features/lightbox/LightboxMediaStage.tsx), imageControls tests | [Batch 07](#batch-07-verification-2026-10-12); controlled detached-image load in WebKit and disposed-owner tests passed. Actual decode-delay incidence is unmeasured. |
| DR-015 | Lightbox favorite epoch ownership; createLightbox tests | [Batch 05](#batch-05-completed-2026-10-12); reused-ID and old/new pending completion tests passed with controlled adapters. |
| DR-016 | [Native surface](../../../src-tauri/src/video_surface.rs), [video service](../../../src-tauri/src/services/video_player_service.rs), [native session](../../../src/features/lightbox/video/createNativeVideoSession.ts), video lifecycle tests | [Batch 11](#batch-11-verification-2026-10-17); seven physical-input cases passed. Retired sessions and pending deletion use controlled tests; video picture quality is not established. |
| DR-018 | Scan model and [thumbnailStop tests](../../../src/features/settings/__tests__/thumbnailStop.test.tsx) | [Batch 12](#batch-12-verification-2026-10-17); 38 controlled cases and rendered controls passed. Delayed Stop replies are not injected in native rendering. |
| DR-019 | [Nine locale resources](../../../src/i18n/locales/), [localization tests](../../../src/features/search/__tests__/localization.test.tsx) | [Batch 13](#batch-13-verification-2026-10-17); eleven-locale snapshots, all four errors and structural checks passed. No independent native-speaker or speech-output review. |
| DR-020 | [i18n core](../../../src/i18n/core.ts), [theme](../../../src/ui/dom/theme.ts), preferenceStorage and theme tests | [Batch 02](#batch-02-verification-2026-10-11); real frontend mounting passed injected storage faults and ordinary persistence. Native failure incidence remains unmeasured. |
| DR-022 | [Gallery performance spec](../../../e2e/specs/gallery.performance.e2e.js) | [Batch 09](#batch-09-completed-2026-10-17); full four-case suite ran in declared order. Filesystem provenance distinguishes first use from revisit; OS-cold caching is not claimed. |

No required repair check remains pending. The record does not claim Wayland, release
packaging, hardware-keyboard coverage, screen-reader speech, independent native-speaker
review, storage-failure incidence or million-row SQLite performance. The retained batch
records distinguish injected failures from ordinary native behavior. The original review
remains historical evidence; this repair record owns the completed dispositions.

A read-only decision-trail review by `gpt-6-astra` checked the raw results, diagnostic
samples, archived scale evidence, profile cleanup and historical hashes. It found no
remaining misleading closure claims. Its [review](../../../raports/20261017-batch-14/trail-review.txt)
notes that desktop passes span multiple invocations and the drawer-failure cause remains
a hypothesis. No active-workspace transcript was available, so the audit checks logs and
artifacts rather than transcript completeness.

## Follow-up native input repair, 2026-10-17

A controlled native regression exposed a remaining failure in the test helper introduced
above. After the pointer rests on a floating button until its opacity reaches zero and
pointer input becomes disabled, moving to the same pixel does not reveal it. The existing
activity model deliberately ignores unchanged pointer coordinates. The helper waited for
a hit-testable button without generating the movement required to reveal it.

`clickDom` in [native-video-escape.e2e.js](../../../e2e/specs/native-video-escape.e2e.js)
now approaches from an adjacent pixel before moving to the target. The new regression
waits for the real inactivity fade with the native pointer already over each button.
It then opens the drawer without replacing the video session and closes the preview.
The pre-fix helper fails at the drawer trigger; the corrected helper passes both actions.
This establishes the controlled stationary-pointer failure without retroactively assigning
a cause to the earlier uninstrumented WebDriver click.

The [testing guide](../../development/testing.md) records the helper's movement requirement.
Application behavior and its inactivity timer are unchanged. Evidence is stored under
[`raports/20261017-input-followup/`](../../../raports/20261017-input-followup/), including the
baseline failure, source comparison and native observations. The recovered screenshot was
inspected and shows the gallery after preview closure.

Verification commands:

```sh
# Expected exit 1 with the pre-fix click helper: stationary-pointer regression fails.
FOLLOWUP_GREP='reveals idle controls' python3 raports/20261017-input-followup/desktop.py baseline native-video-escape
# Exit 0: all eight specs, 44 cases, in one invocation with all six opt-in flags.
python3 raports/20261017-input-followup/desktop.py combined
bun run lint
node --check e2e/specs/native-video-escape.e2e.js
python3 raports/20261017-input-followup/verify.py
git diff --check
```

The combined run passed 44/44 cases without reruns, including both selection-recovery
cases and eight native-video cases. Its preparation passed TypeScript, a fresh frontend
build and the isolated Tauri build. Lint and JavaScript syntax checks passed. The
`verify.py` check compares the expected baseline failure with the single successful
invocation and verifies both idle-control observations. This supersedes the earlier
limitation that passing desktop evidence came only from separate runs.

`cleanup.json` confirms removal of both temporary profiles after checking for owning
processes and mounts. Raw logs and current-run screenshots/JSON are preserved in the
follow-up evidence directory. Existing host graphics/accessibility/D-Bus warnings remain;
a passing test run is not a claim that those host diagnostics disappeared. The earlier
Wayland, release-packaging and other verification limits still apply. No worktree, commit
or push was created, and prior repairs remain intact.

## Full-suite follow-up, 2026-10-17

The broader verification requested after the repair batches is not fully green.
All 761 frontend tests, 212 regular Rust tests, both opt-in Rust diagnostics,
43 tooling tests and 10 native WebKit gallery-scale checks passed. Type checking,
lint, formatting, Clippy, build verification, locale validation, JavaScript audit
and the repository privacy check passed. Packaging/privacy tests required the
pinned Gitleaks 8.30.1 on PATH and passed after that prerequisite was supplied.

The full desktop run enabled every opt-in flag across all 24 specification files:
138 cases passed and six failed. The failures wait for visible tag suggestions in
`lightbox-images.e2e.js` (one), `review-fixes.e2e.js` (one) and
`ui.redesign.e2e.js` (four theme/width combinations). All six failed again in a
fresh profile when selected alone. Their application-versus-driver cause remains
unresolved. The separate real reduced-motion run passed all 46 cases across
animations, thumbnail fade, lightbox activity and settings maintenance.

The Rust dependency audit failed on `zip 2.4.2` with RUSTSEC-2026-0367,
RUSTSEC-2026-0368 and RUSTSEC-2026-0369. This run did not assess reachability or
change dependencies. These failures remain follow-up work; passing the original
repair acceptance cases does not imply a passing full suite.

[Results and logs](../../../raports/20261017-all-tests/RESULTS.md) remain ignored
local evidence. All three desktop profiles were removed after checking for owning
processes and mounts; the scale runner removed its profile and stopped its server.
Current Flatpak/AppImage installation tests and Wayland remain unverified.
