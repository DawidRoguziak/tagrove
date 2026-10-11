# Solid 2 review progress

Status: stages 00 through 02 complete; overall review partial.
Current stage: none active; stage 03 is next.
Current assignment: none; stage 02 reviews, peer handoffs and independent verification are complete.
Next action: recheck source hashes, then begin stage 03, bootstrap and application lifetime.

Review baseline: `ad277bd53b23201af297bccc1e454272d8f3d349`, captured from a clean working tree on 2026-10-10.
Active report directory: `raports/20261010T182819Z-solid2-deep-review/`.
Runtime/build owner: coordinator `/root`; no desktop session was started.

| Stage | Status | Coverage / verification |
| --- | --- | --- |
| 00 | complete | Two independent inventories, peer exchange, 241-file map, hash checks and independent documentation verdict |
| 01 | complete | Dual 116-path semantics review, peer exchange, 359 tests and RC/compiler/build probes |
| 02 | complete | Dual 82-path reliability review, peer exchange, 625 tests plus shuffled run, independent P3 false-pass finding |
| 03 | pending | Awaiting investigation |
| 04 | pending | Awaiting investigation |
| 05 | pending | Awaiting investigation |
| 06 | pending | Awaiting investigation |
| 07 | pending | Awaiting investigation |
| 08 | pending | Awaiting investigation |
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

## Exact resume action

Run `python3 raports/20261010T182819Z-solid2-deep-review/verify_inventory.py` and inspect drift before assigning stage 03. Read [stage 03](03-bootstrap-application-lifetime.md), system architecture and frontend lifetime contracts. Start two independent substantive perspectives on initialization, view switching, startup/listener disposal, lazy-import failure/retry, HMR and window controls. Preserve initial reviews before peer exchange and assign every distinct candidate a fresh verifier. Carry DR-001 and DR-002 forward without applying fixes during this read-only review.
