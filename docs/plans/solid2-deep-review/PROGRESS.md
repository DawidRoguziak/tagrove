# Solid 2 review progress

Status: stages 00 through 08 complete; overall review partial.
Current stage: none active; stage 09 is next.
Current assignment: none; stage 08 reviews, peer exchange, verification and audit are complete.
Next action: recheck source hashes, then begin stage 09, tag editing, bulk actions and groups.

Review baseline: `ad277bd53b23201af297bccc1e454272d8f3d349`, captured from a clean working tree on 2026-10-10.
Active report directory: `raports/20261010T182819Z-solid2-deep-review/`.
Runtime/build owner: coordinator `/root`; isolated stage 08 WebKit fixtures finished and cleaned; no full Tauri session ran.

| Stage | Status | Coverage / verification |
| --- | --- | --- |
| 00 | complete | Two independent inventories, peer exchange, 241-file map, hash checks and independent documentation verdict |
| 01 | complete | Dual 116-path semantics review, peer exchange, 359 tests and RC/compiler/build probes |
| 02 | complete | Dual 82-path reliability review, peer exchange, 625 tests plus shuffled run, independent P3 false-pass finding |
| 03 | complete | Dual 18-path lifetime review, peer exchange, isolated desktop smoke and independent P2 startup-remount finding |
| 04 | complete | Dual eight-path review, peer exchange, native/frontend probes and two independently confirmed P2 findings |
| 05 | complete | Dual 18-path review, peer exchange, 19 supplemental probes and one independently confirmed P2 finding |
| 06 | complete | Dual 21-path review, 100k/1m probes, native WebKit checks and three independent findings |
| 07 | complete | Dual 13-path review, peer exchange, 48 frontend/47 native tests and 12 supplemental cases; no new candidates |
| 08 | complete | Dual 40-path review, peer exchange, native worker/input checks and two independently confirmed P2 findings |
| 09 | pending | Awaiting investigation |
| 10 | pending | Awaiting investigation |
| 11 | pending | Awaiting investigation |
| 12 | pending | Awaiting investigation |
| 13 | pending | Awaiting investigation |
| 14 | pending | Awaiting investigation |
| 15 | pending | Awaiting investigation |

## Stage 00 evidence and limits

[Report](../../../raports/20261010T182819Z-solid2-deep-review/report.md),
[run record](../../../raports/20261010T182819Z-solid2-deep-review/run.json),
[file map](../../../raports/20261010T182819Z-solid2-deep-review/coverage-map.json),
[coverage/hash check](../../../raports/20261010T182819Z-solid2-deep-review/evidence/inventory-check.json).
These artifacts are local and ignored; this summary is the tracked handoff.

The baseline contains 594 file hashes. Final frontend scope is 232 `src/` files and nine root configuration/entry files. Every included file has primary ownership and appears in planned coverage. Peer exchange added `.nvmrc`, which the original eight-config inventory omitted, and expanded shared API, mutation, runtime-test and thumbnail caller links. `rust-toolchain.toml` is supporting. The initial 240-file independent inventories remain preserved.

Independent discovery identities were `/root/baseline_ownership`, requested model `gpt-6-luna`, and `/root/baseline_verification_map`, requested model `gpt-6.1-sol`, both with high reasoning. They saved initial notes before the two-way peer exchange. Discovery does not count toward substantive review coverage in stages 01-15. Observed model metadata was unavailable.

Installed Solid runtime, renderer and compiler are `2.0.0-rc.13`; Vite plugin is `3.0.0-next.47`, testing library `1.0.0-beta.3`, Bun `1.4.0`, Node `26.7.0` and Rust/Cargo `1.98.0`. Node meets the package engine but differs from `.nvmrc`'s `22.22.0`; assess reproducibility in stage 01. The inventory found 74 frontend test files, seven Rust integration targets and 19 desktop specs, 13 of them opt-in. These are discovered files/targets, not executed test counts.

Executed checks included installed package/tool probes, source inventory and hash comparison, the existing `validateE2eBuildConfiguration()` guard, locked offline Cargo metadata, the review-record validator and `git diff --check`. Coverage has no missing paths; application, test and configuration hashes are unchanged. The only tracked edit is this progress file. The E2E guard accepted the isolated identifier, title and target, without building, cleaning or launching the app.

One pre-existing P3 documentation finding, DR-001, was confirmed by fresh verifier `/root/verify_baseline_doc`, requested model `gpt-6.1-sol` with high reasoning. `docs/development/testing.md:27` says two Rust integration binaries; Cargo metadata discovers seven. The backend script is unrestricted and correct; ignored diagnostics still need explicit opt-in. Suggested correction is to describe all discovered integration binaries without a static count. No fix was applied. Confirmed P0/P1/P2: zero; rejected: zero; unresolved candidates: zero.

No application tests, typecheck, lint, locale check, production build, desktop workflow or native rendering check ran in this inventory stage. No application code or existing tests were edited, and no worktree or commit was created. The record validator accepts the run as partial because stages 01-15 remain pending. No previous review results were credited.

## Stage 01 evidence and limits

Completed on 2026-10-10 at `f0d8da3`, with no application, existing-test, configuration or dependency drift from the original baseline. Both substantive reviewers inspected all 116 stage 01 paths. Reviewer `/root/stage01_semantics` covered reactive correctness and installed RC behavior; `/root/stage01_toolchain` covered compiler compatibility, ownership architecture and test assumptions. Both requested `gpt-6.1-sol` with high reasoning; observed model metadata was unavailable. Their initial notes/findings were saved and hashed before the two-way peer exchange, then preserved unchanged.

[Updated report](../../../raports/20261010T182819Z-solid2-deep-review/report.md),
[semantics review](../../../raports/20261010T182819Z-solid2-deep-review/agents/stage01-semantics/notes.md),
[toolchain review](../../../raports/20261010T182819Z-solid2-deep-review/agents/stage01-toolchain/notes.md),
[command record](../../../raports/20261010T182819Z-solid2-deep-review/evidence/stage01-commands.json),
[initial review/path accounting](../../../raports/20261010T182819Z-solid2-deep-review/evidence/stage01-initial-reviews.json).

No stage 01 candidates were submitted or added during peer challenge, so no new independent candidate verdict was needed. Cumulative confirmed counts remain P0 0, P1 0, P2 0, P3 1, with rejected 0 and unresolved 0. DR-001 remains the stage 00 documentation finding; no fix was applied.

Executed verification:

- TypeScript checks and production build/module assertions passed on host Node `26.7.0` and installed pinned Node `22.22.0`. Each build transformed 272 modules and emitted a 193-module application inventory plus a separate worker, with the same reported chunk names/sizes. Browser production Solid/signals/web modules resolved; no React or legacy tree entered the application graph.
- The 32 scoped test files passed 312 tests. Supporting lightbox-model and settings-operation-safety files passed 47 more, for 359 tests across 34 files. These establish bounded frontend command, prop and lifecycle behavior with mocked platform calls.
- Direct installed-RC probes passed development/production browser conditions on Node 26 and production conditions on Node 22. Assertions covered delayed signal reads, functional-write composition, cleanup before reapply, queued-apply suppression after disposal and disposal before first apply.
- A native compiler probe transformed all 68 scoped TSX files and checked generated runtime helper exports. A compiled synthetic DOM flow exercised reactive object props under one retained child owner, indexed list content updates, conditional removal and outer-owner cleanup. Development/production Node 26 and production Node 22 passed; the coordinator independently reran production Node 26.

Peer exchange narrowed the executable claims. The list probe does not instrument row identity, its cleanup counter belongs to the outer child, and helper export presence does not prove every helper's semantics. Installed owner/map/effect source and application lifecycle tests provide additional bounded evidence. An early synthetic probe used the wrong indexed-item access form; only that ignored fixture was corrected. It was not an application defect.

Final inventory/source-hash checks, initial-note hash preservation, review-record validation and `git diff --check` passed. All new evidence stays in the ignored run directory; only this progress file changed among tracked files. No worktree, commit or push was created.

No desktop session, native rendering, HMR, backend suite, lint, locale check or full frontend suite ran in stage 01. Detailed feature behavior, test reliability and integrated runtime checks remain assigned to later stages. The overall report is partial because stages 02-15 are pending.

## Stage 02 evidence and limits

Completed on 2026-10-11 with no application, existing-test, E2E, script, configuration or dependency drift from the original baseline. Two fresh reviewers inspected all 82 stage paths. `/root/stage02_scheduling` examined reactive scheduling, same-owner rerender, deferred completions and cleanup. `/root/stage02_harness` examined test architecture, mock fidelity, isolation and E2E support. Both requested `gpt-6.1-sol` with high reasoning; observed model metadata was unavailable. Inspection depth is recorded explicitly, including targeted excerpts in long scheduling suites and supporting controller/spec files.

[Updated report](../../../raports/20261010T182819Z-solid2-deep-review/report.md),
[scheduling review and completion addendum](../../../raports/20261010T182819Z-solid2-deep-review/agents/stage02-scheduling/review-addendum.md),
[harness review](../../../raports/20261010T182819Z-solid2-deep-review/agents/stage02-harness/notes.md),
[initial artifact hashes and scope accounting](../../../raports/20261010T182819Z-solid2-deep-review/evidence/stage02-initial-reviews.json),
[required command record](../../../raports/20261010T182819Z-solid2-deep-review/evidence/stage02-commands.json),
[support checks](../../../raports/20261010T182819Z-solid2-deep-review/evidence/stage02-support-commands.json).

One new P3 test-reliability defect, DR-002, was independently confirmed by fresh verifier `/root/verify_stage02_disposal`, requested `gpt-6.1-sol` with high reasoning. The negative assertion in `src/app/__tests__/runtime.test.ts:63` runs before the startup response handler and Solid publication have completed. All three original runtime tests still pass when an ignored Vite transform removes the disposal guard in memory. The verifier checked the exact transform and instrumented handler entry/exit. Natural microtasks then publish the late value without extra flushes. A corrected diagnostic assertion fails against the mutation; unchanged production retains the empty snapshot.

Current production disposal handling is correct. This finding concerns a regression test that can report success without testing its stated behavior. Both adapter promise adoption and signal batching matter; a direct-return adapter mutant also passes the original assertion. Repair must await consumer completion and publication before checking absence. An already-true `waitFor` assertion is insufficient. [Independent verdict and evidence](../../../raports/20261010T182819Z-solid2-deep-review/verification/DR-002.json). No fix was applied.

Cumulative confirmed counts are P0 0, P1 0, P2 0, P3 2; rejected 0; unresolved 0. DR-001 remains the stage 00 documentation finding. No application defect has been demonstrated in the completed scope.

Executed verification:

- Typecheck passed for both projects; lint checked 254 files without fixes; locale validation passed for 11 languages.
- All 625 frontend tests across 74 files passed. A full shuffled run with seed `20261011` also passed 625 tests; the reviewer preserved its earlier focused 140-test run separately. One seed does not exhaust possible schedules.
- Production build/module assertions passed, recording 193 application modules with the Solid entry and no forbidden dependencies.
- Four disposable fake-build checks passed. All six controller support checks passed with local socket access, including process ownership/cleanup and synthetic media validation with ffprobe. The sandbox attempt failed specifically on localhost `EPERM`; an approved unrestricted rerun passed. No desktop app was launched.
- Helper probes confirmed that `renderSolid` retains its component owner and DOM node across prop changes. `renderSettings.rerender` replaces its child subtree; its sole current rerender caller checks separate lock states and does not claim draft continuity.
- Independent mutation probes reproduced DR-002 and distinguished adapter completion from signal publication. The intentional failing diagnostic is evidence of mutation sensitivity, not a failure in unchanged application source.

Both peer handoffs are answered; initial notes, findings and scope records remain unchanged after exchange. Source/hash checks, review-record validation and `git diff --check` passed. All new probes, logs and review artifacts stay ignored under the run directory. Only this progress file changed among tracked files. No worktree, commit or push was created.

No desktop session, HMR, native rendering, backend suite, live IPC/database workflow or release package verification ran in stage 02. Observer and worker mocks retain their documented limits. The overall review remains partial because stages 03-15 are pending.

## Stage 03 evidence and limits

Completed on 2026-10-11 with no application, existing-test, configuration or dependency drift from the original baseline. Two fresh reviewers inspected all 18 stage paths. `/root/stage03_lifecycle` traced reactive initialization, asynchronous registration and disposal. `/root/stage03_architecture` traced application ownership, recovery and native startup contracts. Both requested `gpt-6.1-sol` with high reasoning; observed model metadata was unavailable. Initial notes, findings and inspection records were saved and hashed before peer exchange, then preserved unchanged.

[Updated report](../../../raports/20261010T182819Z-solid2-deep-review/report.md),
[lifecycle review](../../../raports/20261010T182819Z-solid2-deep-review/agents/stage03-lifecycle/notes.md),
[architecture review](../../../raports/20261010T182819Z-solid2-deep-review/agents/stage03-architecture/notes.md),
[coverage and hash check](../../../raports/20261010T182819Z-solid2-deep-review/evidence/stage03-coverage-check.json),
[command record](../../../raports/20261010T182819Z-solid2-deep-review/evidence/stage03-commands.json).

One new P2 defect, DR-003, was independently confirmed by `/root/verify_stage03_startup`, requested `gpt-6-astra` with high reasoning. Reloading the frontend after an automatic startup scan dispatches but before it completes can leave the replacement gallery empty or partially populated. The backend has consumed its once-only snapshot, so the replacement startup call returns null. Its progress subscription ends; the disposed original runtime cannot refresh the replacement query when scanning finishes. Empty or fully cached views do not request a page that could detect the stale revision. A new query or reload after completion recovers the results.

The verifier's four-case probe awaits the actual production startup consumer through follow-up work and cleanup. Empty and two-row results remain stale after completion, then recover to seven rows on explicit refresh. Ordinary Settings/gallery switching preserves the runtime and works; remount after completion also works. Rust source confirms that the second startup call returns before the scan lock, and query admission does not wait for scanning. The active-scan race was not executed in a live native session. [Independent verdict](../../../raports/20261010T182819Z-solid2-deep-review/verification/DR-003.json).

Repair direction is process-owned startup status/completion that replacement runtimes can observe without duplicating scanning. No fix was applied. Owning source matches the original baseline, so the defect is pre-existing relative to this review. Cumulative confirmed counts are P0 0, P1 0, P2 1, P3 2; rejected 0; unresolved 0. DR-001 and DR-002 remain unchanged.

Executed verification:

- Eight focused app/startup test files passed 36 tests. Four strengthened lifecycle probes passed, covering pending startup across view changes, late registration cleanup, disposed completions and Solid lazy-thunk rejection/retry. Two strengthened architecture probes also passed. The independent verifier passed four additional cases with a directly awaited consumer-completion barrier.
- Review probe configurations initially concatenated root include globs and ran the existing suite alongside probes. Architecture passed 627 tests; the verifier passed 629. Lifecycle passed 628 cases with one synthetic-probe failure from a wrong button label. Corrected focused runs passed. Preserved logs and addenda distinguish these setup mistakes from application results.
- The isolated controller passed typecheck, frontend production build and Tauri E2E build. Doctor verified the E2E title, disposable profile, sole synthetic root and private display. Settings opened, Escape retained the gallery search draft, an ordinary reload showed seven indexed items with the nonpersistent draft cleared, and Back returned from Settings. Both screenshots were opened and inspected. Repeated stop succeeded; direct PID and filesystem checks confirmed cleanup.
- Peer exchange strengthened negative async assertions to wait for consumer completion. Both handoffs are answered. Source/hash checks, preserved initial artifact hashes, coverage accounting, review-record validation and `git diff --check` passed.

[Desktop evidence](../../../raports/20261010T182819Z-solid2-deep-review/evidence/stage03-desktop.md),
[cleanup check](../../../raports/20261010T182819Z-solid2-deep-review/evidence/stage03-cleanup-check.json),
[decision trail and audit](../../../raports/20261010T182819Z-solid2-deep-review/agents/stage03-trail-audit/trail-audit.md).
The trail auditor requested `gpt-5.6-sol`; observed model telemetry and a conversation transcript were unavailable, so the audit checks persisted evidence without claiming verified cross-family execution.

No live active-scan reload, Vite HMR delivery or failed production chunk download ran. Lazy retry used the installed Solid runtime with a synthetic thunk. Bare Xvfb accepted a Maximize click but did not establish a native state transition; minimize, drag/resize and close remain untested. No native video pixels, backend suite or release package verification ran. All generated evidence remains ignored. Only this progress file changed among tracked files; no worktree, commit or push was created. The overall review remains partial because stages 04-15 are pending.

## Stage 04 evidence and limits

Completed on 2026-10-11 with no application, existing-test, configuration or dependency drift from the original baseline. Two fresh reviewers inspected all eight assigned paths. `/root/stage04_frontend` traced reactive state, async ordering and sparse cache behavior across 26 paths. `/root/stage04_contracts` traced Rust/SQLite guarantees and frontend contract fidelity across 32 paths. Both requested `gpt-6.1-sol` with high reasoning; observed model metadata was unavailable. Their initial notes, findings and inspection records were saved and hashed before peer exchange, then preserved unchanged.

[Report](../../../raports/20261010T182819Z-solid2-deep-review/report.md),
[frontend review](../../../raports/20261010T182819Z-solid2-deep-review/agents/stage04-frontend/notes.md),
[contract review](../../../raports/20261010T182819Z-solid2-deep-review/agents/stage04-contracts/notes.md),
[coverage and hash checks](../../../raports/20261010T182819Z-solid2-deep-review/evidence/stage04-coverage-check.json),
[coordinator commands](../../../raports/20261010T182819Z-solid2-deep-review/evidence/stage04-commands.json).

Two new P2 defects were independently confirmed. Both predate the supplied review baseline; no fixes were applied.

- DR-004: overlapping frontend queries can register in reverse order inside independently scheduled Tauri async command bodies. The backend accepts the older frontend request and supersedes the newer one. Frontend generation guards then discard the old ready response, leaving retained rows but no usable session or error until another explicit refresh. Fresh verifier `/root/verify_stage04_order`, requested `gpt-6-astra` with high reasoning, observed 51 registration inversions in 30,000 sequentially submitted native task pairs without inserted waits. The production query service returned older ready/newer superseded for an observed inversion. Three independent frontend cases checked both response orders, recovery and normal reversed-completion controls. This is not a desktop incidence estimate. [Verdict](../../../raports/20261010T182819Z-solid2-deep-review/verification/DR-004.json).
- DR-005: a single ID vector exceeding the intentional 64 MiB session budget returns successful superseded even when its request token and cache epoch remain current. Initial load looks empty; previously cached summaries remain readable, while new-page and position reads lack a session. Fresh verifier `/root/verify_stage04_capacity`, requested `gpt-6.1-sol` with high reasoning, passed three native and three frontend cases. A nine-row/128-byte vector reproduced the classification at a 127-byte test budget and worked at 129 bytes. The actual production guard accepted capacity 8,388,608 i64 entries and rejected 8,388,609. More than 8,388,608 matches guarantees failure; no smaller exact row cutoff was established. Narrower filters can recover. Preserve the budget and return an actionable capacity error. [Verdict](../../../raports/20261010T182819Z-solid2-deep-review/verification/DR-005.json).

Cumulative confirmed counts are P0 0, P1 0, P2 3, P3 2; rejected 0; unresolved 0. DR-001 through DR-003 remain unchanged.

Executed verification:

- Four focused frontend files passed 48 tests. Native query-manager tests passed 14; database tests passed 52, including snapshot, revision, ordering, cancellation and rollback checks.
- Eight frontend-review probes passed, covering reversed starts, loading counters, repeated metadata patches, positions after eviction, complete ranges and pending-start disposal. Two contract-review probes and six independent verifier frontend probes passed. Native verifier evidence is described above.
- The contract review initially concatenated root test includes and passed all 625 existing frontend tests plus two probes, 627 across 75 files. A corrected config then passed exactly two cases. The ignored run's `run_probe.py` now compares discovered files with the expected list before execution. Its guard accepted the one-file probe and refused the root 74-file suite without running tests.
- Both peer handoffs are answered. Coverage and source-hash checks, preservation of six initial artifacts, and review-record validation passed. Direct path checks confirmed the native order-probe database and WAL/SHM files were removed. Capacity fixtures use TempDir cleanup; their paths were not logged, so removal was not independently checked by path.

No live WebView query race, desktop session, multi-million-row SQLite query, production-scale performance measurement, full backend suite or release verification ran. Native scheduling/service and frontend model results are separate executed layers, connected by inspected dispatch and IPC contracts. No typecheck, lint or locale validation ran in this stage. All generated evidence stays ignored; only this progress document changed among tracked files. No worktree, commit or push was created.

The [decision trail](../../../raports/20261010T182819Z-solid2-deep-review/evidence/stage04-decisions.tsv) and [audit](../../../raports/20261010T182819Z-solid2-deep-review/agents/stage04-trail-audit/trail-audit.md) retain the discovery-scope clarification and verification limits. The auditor requested `gpt-5.6-sol`; observed model metadata and a conversation transcript were unavailable. It found no remaining factual trail discrepancy. Overall review remains partial because stages 05-15 are pending.

## Stage 05 evidence and limits

Completed on 2026-10-11 with two independent reviews, peer exchange, fresh candidate verification and trail audit. Application, existing-test and configuration hashes remain unchanged from the original baseline. Both reviewers inspected all 18 stage paths. `/root/stage05_frontend` traced reactive ownership, cache lifetime and canonical publication across 50 paths; `/root/stage05_contracts` traced architecture, Rust/IPC/SQLite contracts and maintenance across 54 paths. Both requested `gpt-6.1-sol` with high reasoning; observed model metadata was unavailable. Six initial review artifacts remain unchanged after peer exchange.

[Report](../../../raports/20261010T182819Z-solid2-deep-review/report.md),
[frontend review](../../../raports/20261010T182819Z-solid2-deep-review/agents/stage05-frontend/notes.md),
[contract review](../../../raports/20261010T182819Z-solid2-deep-review/agents/stage05-contracts/notes.md),
[coverage and hashes](../../../raports/20261010T182819Z-solid2-deep-review/evidence/stage05-coverage-check.json),
[coordinator commands](../../../raports/20261010T182819Z-solid2-deep-review/evidence/stage05-commands.json).

One new P2 defect, DR-006, was independently confirmed by `/root/verify_stage05_details`, requested `gpt-6.1-sol` with high reasoning. A pending metadata save captures a complete details record before IPC. If Settings rescan or duplicate rename refreshes the query before the save settles, the save restores that older record with a fresh metadata token. Reopening the lightbox accepts stale path or size without another details request, even though the gallery has current summaries. A successful rename can therefore leave the lightbox reading the former pathname. The frontend stale-record behavior was reproduced; a physical rename and WebView load failure were not executed.

The verifier's eight independent cases exercise actual lightbox save, Settings controllers, query/cache and reopened selection with controlled IPC. Three reproduction schedules pass, including tag commit before duplicate discovery with delayed response, and tag execution after rename. Five controls pass: ordinary save, identity reset, pre-save eviction, same-save refresh and unchanged-file query clear. Duplicate revision guards do not reject either modeled valid schedule. Source traces establish backend feasibility; live Tauri reply ordering and incidence remain unmeasured. [Verdict](../../../raports/20261010T182819Z-solid2-deep-review/verification/DR-006.json).

Repair direction is to preserve the details-cache generation at mutation start and avoid restoring a captured full record after a clear, while still publishing canonical metadata. No fix was applied. The source matches the original baseline. Cumulative confirmed counts are P0 0, P1 0, P2 4, P3 2; rejected 0; unresolved 0. DR-001 through DR-005 remain unchanged.

Executed verification:

- Coordinator frontend suite passed 167 tests across 10 files. Database unit tests passed 52; backend integration/workflow tests passed seven; database-pool tests passed three.
- Frontend reviewer passed 86 tests across seven assigned files, overlapping the coordinator suite, plus five supplemental cases. Contract reviewer passed six supplemental cases. Fresh verifier passed eight cases. These cover reversed completions, partial canonical responses, save after details eviction, concurrent lightbox/bulk writes with maintenance, identity reset and the stale-cache finding.
- Both reviewers initially merged extra test include patterns. The exact-file discovery guard prevented unintended suite execution. The frontend refusal has a raw log; the contract refusal is recorded in reviewer notes/commands only. A maintenance assertion and a missing Settings mock dependency were corrected in ignored probes; their failing runtime logs remain preserved.
- Both peer handoffs are answered. Coverage and initial-artifact hash checks passed. Final inventory, record validation and `git diff --check` passed.

No desktop session, live IPC response-order experiment, physical rename/rescan reproduction, native video, full backend suite, release build, typecheck, lint or locale validation ran in stage 05. Frontend tests and native persistence tests are separate executed layers. No application, existing-test or configuration changes were made. All generated evidence remains ignored; only this progress document changed among tracked files. No worktree, commit or push was created.

The [decision trail](../../../raports/20261010T182819Z-solid2-deep-review/evidence/stage05-decisions.tsv) and [audit](../../../raports/20261010T182819Z-solid2-deep-review/agents/stage05-trail-audit/trail-audit.md) record the missing raw contract-discovery refusal log and distinguish inventory evidence from assignment evidence. These were clarified without changing initial reviews. Requested auditor model was `gpt-5.6-sol`; observed model metadata and a conversation transcript were unavailable. The audit found no remaining factual mismatch. The overall review remains partial because stages 06-15 are pending.

## Stage 06 evidence and limits

Completed on 2026-10-11 with peer exchange, three independent verdicts and final trail audit. Two fresh reviewers inspected all 21 stage paths. `/root/stage06_behavior` traced user interaction, reactive identity and selection across 42 paths; `/root/stage06_scale` traced virtualization, bounded work and cache coordination across 44 paths. Both requested `gpt-6.1-sol` with high reasoning; observed model metadata was unavailable. Six initial reviewer artifacts and two coordinator intake artifacts remain unchanged after peer exchange. Application, existing-test and configuration hashes match the original baseline.

[Report](../../../raports/20261010T182819Z-solid2-deep-review/report.md),
[behavior review](../../../raports/20261010T182819Z-solid2-deep-review/agents/stage06-behavior/notes.md),
[scale review](../../../raports/20261010T182819Z-solid2-deep-review/agents/stage06-scale/notes.md),
[coverage and hashes](../../../raports/20261010T182819Z-solid2-deep-review/evidence/stage06-coverage-check.json),
[coordinator commands](../../../raports/20261010T182819Z-solid2-deep-review/evidence/stage06-commands.json).

Three pre-existing findings were independently confirmed by separate fresh `gpt-6.1-sol` verifiers, all with high reasoning and unavailable observed model metadata. No fixes were applied.

- DR-007, P2: a single full-height gallery spacer exceeds WebKit's coordinate limit. At one million results, five 188px columns require 39,999,988px; native WebKitGTK 2.54.1 clamps the spacer to 33,554,428px. Repeated maximum scrolling leaves the mounted/requested range ending at index 838,884, without final index 999,999. Smaller tiles, wider grids and narrower result counts recover. A 100k result set also fails at one 588px column, so the threshold depends on layout. Fresh verifier `/root/verify_stage06_height` passed nine native cases and 11 assertions against actual GalleryGrid, virtual-core and checked production CSS. Repair requires bounded physical scroll coordinates mapped to logical indices. [Verdict](../../../raports/20261010T182819Z-solid2-deep-review/verification/DR-007.json).
- DR-008, P2: an error banner moves the grid origin and handler cleanup erases a settled failed-selection retry. Fresh verifier `/root/verify_stage06_retry` anchored an early tile, browsed 15 pages, Shift-clicked a loaded late tile and failed evicted intermediate page 128. The native origin moved from 20 to 74px; clicking visible Retry cleared the error without issuing another page request. Prior IDs remained intact. An unchanged-origin control completed all 1,953 IDs; viewport retry and newer-input controls passed. Four native cases and 11 assertions passed. Preserve failed retry intent when cancelling inactive gesture state during layout updates. [Verdict](../../../raports/20261010T182819Z-solid2-deep-review/verification/DR-008.json).
- DR-009, P3: the gallery guide says production uses `hasMore: false`, but AppGalleryView passes `offset < total`. Footer fallback/guard descriptions also contain stale text. Fresh verifier `/root/verify_stage06_docs` confirmed the exact code/document mismatch without inferring a runtime defect. [Verdict](../../../raports/20261010T182819Z-solid2-deep-review/verification/DR-009.json).

Cumulative confirmed counts are P0 0, P1 0, P2 6, P3 3; rejected 0; unresolved 0. DR-001 through DR-006 remain unchanged.

Executed verification:

- The focused existing suite passed 103 tests across 10 files, including 10,000/26,000-slot cases. Reviewers passed eight behavioral probes, two scale probes and two peer counterexamples, 12 supplemental tests in total.
- Metadata-only 100k/1m scenarios used real GalleryGrid, library and query owners with controlled IPC. Both mounted at most 60 slots, retained at most 1,536 summaries and issued at most two page reads per jump, 38 over the route. All three observer registrations disconnected on disposal. The measurement cache still scales with row count. Mocked geometry cannot establish native scroll limits or desktop latency.
- Native fixtures compiled production components and CSS in isolated owned output directories, using ephemeral WebKitGTK, synthetic data and temporary localhost servers. Height evidence records range demand; Retry uses real API/query wrappers with injected invoke replies. These are separate from live Tauri/SQLite execution. Native processes and temporary profiles were cleaned; direct cleanup checks passed.
- Failed probe attempts remain visible. A height-fixture callback returned a number as Solid cleanup; correcting it produced passing evidence. The initial Retry fixture omitted Tailwind utilities because its root was ignored; explicit source discovery and computed-style assertions corrected it. The retained height bundle separately passed its CSS audit. Behavioral fixture corrections are preserved in reviewer logs. No application code changed to make probes pass.
- Both peer handoffs are answered. Coverage, initial-artifact hash checks, source inventory, record validation and `git diff --check` passed.

No full Tauri application session, live Rust bridge, million-row SQLite query, hardware-input or pixel check ran. Native Retry output includes settled ResizeObserver loop notifications and uses fallback fonts. No Rust suite, full frontend suite, production application build, typecheck, lint, locale check or release verification ran in stage 06. All generated artifacts remain ignored; only this progress document changed among tracked files. No worktree, commit or push was created.

The [decision trail](../../../raports/20261010T182819Z-solid2-deep-review/evidence/stage06-decisions.tsv) and [audit](../../../raports/20261010T182819Z-solid2-deep-review/agents/stage06-trail-audit/trail-audit.md) resolve evidence-link gaps for fixture failures, cleanup and cumulative counts. The peer documentation assignment was copied byte-for-byte into its required owned directory after an initial validator rejection. The final audit found no remaining factual discrepancy. Requested auditor model was `gpt-5.6-sol`; no workspace transcript or observed model metadata was available. Native CSS utilities were checked, without claiming byte-identical application bundles. Overall review remains partial because stages 07-15 remain pending.

## Stage 07 evidence and limits

Completed on 2026-10-11 at `3051692`, including independent reviews, peer exchange and final trail audit. Two independent reviewers inspected all 13 assigned paths. `/root/stage07_frontend` traced Solid ownership, demand, subscriptions and tile updates across 44 paths. `/root/stage07_contracts` traced architecture, scheduler admission and Rust/IPC/database publication across 42 paths. Both requested `gpt-6.1-sol` with high reasoning; observed model metadata was unavailable. Six initial review artifacts remain unchanged after peer exchange. Application, existing-test and configuration hashes match the original baseline.

[Report](../../../raports/20261010T182819Z-solid2-deep-review/report.md),
[frontend review](../../../raports/20261010T182819Z-solid2-deep-review/agents/stage07-frontend/notes.md),
[contract review](../../../raports/20261010T182819Z-solid2-deep-review/agents/stage07-contracts/notes.md),
[coverage and hashes](../../../raports/20261010T182819Z-solid2-deep-review/evidence/stage07-coverage-check.json),
[coordinator commands](../../../raports/20261010T182819Z-solid2-deep-review/evidence/stage07-commands.json).

Neither independent investigation nor peer exchange submitted a candidate. No new independent candidate verdict was needed, and no fix was applied. Cumulative confirmed counts remain P0 0, P1 0, P2 6, P3 3; rejected 0; unresolved 0. DR-001 through DR-009 remain unchanged. This result is limited to the inspected paths and executed scenarios.

Executed verification:

- The coordinator's focused frontend suite passed 48 tests across seven files. The frontend reviewer passed 19 overlapping existing tests. Rust checks passed 12 scheduler, 17 service, 17 rendering and one file-backed thumbnail-clear case, 47 total, including real synthetic video probing/rendering and image regeneration after clear.
- Ten supplemental frontend cases passed. Queue cold/warm/eviction request totals were 1/1/2. Twenty-five repeated failed demands added no request until reset. Old ready/failed/stale events and invoke rejection did not repopulate or poison the new generation. Inactive paths/versions were released; mounted subscribers retained paths until unsubscribe.
- Completing one of 100 actual GalleryTile components replaced one image and cleared its spinner; 99 unrelated image nodes retained identity with zero unrelated path reads or media conversions. A separate store probe with 10,000 subscriptions notified the changed ID once and unrelated IDs zero times. These are update counts under controlled frontend execution, not native latency or memory benchmarks.
- Two supplemental native scheduler cases passed using actual scheduler source and controlled processors. Two active browsing videos blocked a third while an eligible low-priority image ran. Admission stopped at 2,048 distinct jobs and 256 waiters per target; all admitted receivers received terminal results. Renderer and database-permit stubs were not exercised in these probes.
- Peer review corrected an evidence claim: the initial 13-page library fixture tested passive path rehydration without gallery demand. A separate follow-up passed with 14 viewport changes across 13 pages and a revisit, then explicit demand for returned IDs. It retained 24 synthetic ready summaries, refetched one evicted page and sent zero generation invokes. Initial evidence remains preserved. Prior virtualizer coverage belongs to stage 06; no GalleryGrid/virtualizer test reran here.
- One intermediate probe failed in its observation writer after the six cases passed. Correcting only the owned output path produced passing recorded evidence. A nonexistent coordinator integration-test filter contributed no tests; the saved discovery list identifies the seven actual files. Both peer handoffs are answered, and coverage, source inventory, review-record validation and `git diff --check` passed.

No desktop session, live Tauri channel-order experiment, WebKit rendering/pixel check, cold-video viewport-latency measurement or native memory benchmark ran. Frontend probes use jsdom, fake timers and mocked IPC; native suites establish separate backend behavior. Rust fixtures use temporary directories; their individual cleanup paths were not logged or independently checked. No full frontend/backend suite, production application build, typecheck, lint, locale check or release verification ran in this stage. Only this progress document changed among tracked files. No worktree, commit or push was created.

The [decision trail](../../../raports/20261010T182819Z-solid2-deep-review/evidence/stage07-decisions.tsv) and [audit](../../../raports/20261010T182819Z-solid2-deep-review/agents/stage07-trail-audit/trail-audit.md) retain the peer correction and separate follow-up. The auditor found two narrow evidence pointers, corrected through appended rows linking the inventory and separate supplemental/coverage results. No factual mismatch remains. Requested auditor model was `gpt-5.6-sol`; workspace transcript and observed model metadata were unavailable. Overall review remains partial because stages 08-15 remain pending.

## Stage 08 evidence and limits

Completed on 2026-10-11 at `2cea9aa`, including peer exchange, two independent verdicts and final trail audit. Two independent reviewers inspected all 40 assigned paths. `/root/stage08_behavior` covered input, reactive ownership and user actions across 71 paths; `/root/stage08_protocol` covered worker/vocabulary architecture and backend contracts across 69. Both requested `gpt-6.1-sol` with high reasoning; observed model metadata is unavailable. Six initial review artifacts remain unchanged after peer exchange. Application, existing-test and configuration hashes match the original baseline.

[Report](../../../raports/20261010T182819Z-solid2-deep-review/report.md),
[behavior review](../../../raports/20261010T182819Z-solid2-deep-review/agents/stage08-behavior/notes.md),
[protocol review](../../../raports/20261010T182819Z-solid2-deep-review/agents/stage08-protocol/notes.md),
[coverage and hashes](../../../raports/20261010T182819Z-solid2-deep-review/evidence/stage08-coverage-check.json),
[coordinator commands](../../../raports/20261010T182819Z-solid2-deep-review/evidence/stage08-commands.json).

Two pre-existing P2 findings were confirmed by separate fresh `gpt-6.1-sol` verifiers with high reasoning and unavailable observed model metadata. No fixes were applied.

- DR-010: after suggestion worker failure, clicking Retry blurs the input and removes the button before click. Tab plus Enter also loses the control. Typing and editor remount keep the shared client failure latched. `/root/verify_stage08_retry` reproduced trusted pointer/key ordering in WebKitGTK 2.54.1 through actual SearchTagator, Solid and SuggestionClient with injected worker errors/replies. Failed states render zero options; direct retry and a fixture-only pointer focus guard render the cat option. Ordinary text submission works. Preserve both error state and control lifetime through focus transfer; removing only the JSX focus condition or adding only a pointer guard is insufficient. [Verdict](../../../raports/20261010T182819Z-solid2-deep-review/verification/DR-010.json).
- DR-011: valid constructor and __proto__ tags look included in an empty tag-list selection. Single-click plus Apply replaces the prior filter with an empty search; double-click and Shift+Enter/Space throw a TypeError. Ordinary object membership reads inherited values, and __proto__ string assignment stores no own value. `/root/verify_stage08_selection` passed 17 cases through actual modal/search controls. Ordinary cat selections and typed include/exclude searches work. Rust acceptance is established through source/schema/contracts, not live IPC. Use Map or consistently preserved null-prototype records across lookup, insertion, copying and reset. No security exploitation is claimed. [Verdict](../../../raports/20261010T182819Z-solid2-deep-review/verification/DR-011.json).

Cumulative confirmed counts are P0 0, P1 0, P2 8, P3 3; rejected 0; unresolved 0. DR-001 through DR-009 remain unchanged.

Executed verification:

- Coordinator frontend suite passed 94 tests across 14 files; the behavior reviewer separately passed an overlapping 97 tests across 15 files. Rust checks passed ten database listing/query tests, six vocabulary integration tests and five normalization tests, 21 total.
- Behavior probes passed seven cases plus two peer follow-ups. They cover reversed query replies, draft/applied filters retained across toolbar remount, editor disposal, large Fuse vocabulary and both findings. Remount proves application-lifetime retention, not process-restart persistence. Protocol checks passed seven, including one assertion on a separately executed real Worker artifact. Its other cases exercise failure/staleness, rendered controls and 10,000-tag hydration over 50 mocked pages. Fresh Retry verification adds four frontend cases; fresh selection verification adds 17.
- Actual production worker/client/Fuse passed separate Bun Worker checks at 10,003/100,003 names and six native WebKit cases at up to 50,001 names. Native cases cover ranking/highlights, stale vocabulary/coalescing, disposal and worker error/retry. Single 50,001-name native cold/warm samples were 87/79 ms; these are synthetic observations without desktop responsiveness or memory thresholds.
- Seven final native Retry cases passed with actual suggestion-option assertions. An initial listbox-presence recovery check was too weak because failure can retain an empty listbox. Its output and bundle remain preserved; the final run requires a rendered cat option. The first fixture source was reconstructed after execution without an original pre-build hash; final fixture hashes were captured before build and checked unchanged.
- Peer exchange corrected initial constructor counterevidence: direct helper insertion works, but the modal double-action forwards an inherited function and throws. Two separate follow-up cases verify this and keyboard Retry. The coordinator mentioned overlapping selection candidates after both reviewers independently reported them but before behavior notes were saved; no peer artifacts were shared until both initial reviews were frozen.
- Protocol and selection-verifier failed harness attempts remain saved. Early behavior probe raw logs were overwritten; their failures are explicitly reconstructed in notes. A nested Bun spawn returned EPERM; direct execution succeeded separately. The first native worker run started D-Bus before setting temporary XDG paths; its passing rerun applies isolation before launch. Native launcher/profile/server cleanup checks passed.

No full Tauri session, live bridge or persistence workflow, screenshot/pixel check, physical hardware-input test, production-scale memory benchmark, full frontend/backend suite, application production build, typecheck, lint, locale or release verification ran. Frontend checks use jsdom and controlled IPC; actual worker and native component checks establish separate boundaries. XTest input is synthetic but produces trusted native events. Rust fixture cleanup paths were not individually audited. Only this progress document changed among tracked files. No worktree, commit or push was created.

[Decision trail](../../../raports/20261010T182819Z-solid2-deep-review/evidence/stage08-decisions.tsv),
[audit](../../../raports/20261010T182819Z-solid2-deep-review/agents/stage08-trail-audit/trail-audit.md).
Requested auditor model is `gpt-5.6-sol`; workspace transcript and observed model telemetry are unavailable. The audit found narrow evidence pointers, corrected with appended trail rows. No factual or evidence-integrity flag remains. Final coverage, source inventory, record validation and `git diff --check` passed. Overall review remains partial because stages 09-15 remain pending.

## Exact resume action

Stage 09, tag editing, bulk actions and groups, is next. Recheck source hashes with `python3 raports/20261010T182819Z-solid2-deep-review/verify_inventory.py`. Read [stage 09](09-tag-editing-bulk-actions-groups.md), its owning search/tag/group and metadata contracts, and follow the same dual-review, saved-initial, peer-exchange and independent-verifier workflow. Keep exact-file probe discovery and separate controlled frontend evidence from native runtime claims. Carry DR-001 through DR-011 forward without fixes.
