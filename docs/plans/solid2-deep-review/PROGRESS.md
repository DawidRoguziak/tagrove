# Solid 2 review progress

Current stages: 04-09 independent library and interaction review; 14 desktop verification.
Current assignments: library-a/library-b peer exchange, interaction-b scale review; coordinator desktop suites and user-requested plan checkpoint commit.
Exact next action: finish peer exchanges, start lightbox/settings reviews as slots open, inspect desktop suite results, then independently verify candidates.

Review baseline: `460d431599012ecb779ef4b1c16771b4e79e5692`. Existing edits: lightbox model/test/docs and duplicate resolver component/test/settings docs, six files. These edits are included and preserved.

Active report directory: [raports/20261010T175947Z-solid2](../../../raports/20261010T175947Z-solid2). Runtime/build owner: `/root`.

| Stage | Status | Coverage / verification |
| --- | --- | --- |
| 00 | complete | HEAD, six edits, hashes, dependencies, host and full file inventory captured |
| 01 | in_progress | Independent reviews or baseline capture in progress |
| 02 | in_progress | Independent reviews or baseline capture in progress |
| 03 | in_progress | Independent reviews or baseline capture in progress |
| 04 | in_progress | Independent reviews or baseline capture in progress |
| 05 | in_progress | Independent reviews or baseline capture in progress |
| 06 | in_progress | interaction-a and interaction-b independent investigations |
| 07 | in_progress | Independent reviews or baseline capture in progress |
| 08 | in_progress | interaction-a and interaction-b independent investigations |
| 09 | in_progress | interaction-a and interaction-b independent investigations |
| 10 | pending | Awaiting independent investigation |
| 11 | pending | Awaiting independent investigation |
| 12 | pending | Awaiting independent investigation |
| 13 | in_progress | Independent reviews or baseline capture in progress |
| 14 | in_progress | Repeated desktop cycles and scale probes underway |
| 15 | pending | Awaiting independent investigation |

All stages were initialized pending. Stage starts and completion are recorded below after investigation begins.

## Commands and evidence

- Initializer captured HEAD, six dirty paths and source hashes in run.json.
- Coverage inventory and supporting source hashes saved in coverage.json and run.json.
- No application or existing test files modified.

## Outstanding work

Lightbox/settings dual reviews, remaining peer exchanges, fresh candidate verification and final report remain outstanding. The desktop opt-in suite is running. Baseline checks and the first runtime cycle workload are complete. A passing intermediate record validator does not establish review completion.

## Baseline verification

All five baseline commands passed. `bun run test` ran 625 tests in 74 files. Typecheck, lint, locale validation and production build graph validation also passed. See [command records](../../../raports/20261010T175947Z-solid2/baseline-checks.json) and [environment](../../../raports/20261010T175947Z-solid2/environment.txt).

Desktop controller startup initially failed because sandbox local socket access was denied. Approved escalation started and verified the isolated E2E app. No production profile was started.

## Foundation independent review and handoff

Both independent foundation reviews saved initial records covering the same 67 paths. The first reported 63 in its chat summary; file inventory confirms 67. Foundation-a proposed two conditional storage-failure candidates; foundation-b found no initial candidates and challenges the demonstrated native trigger. Foundation peer exchange is complete. Three scheduler/rerender/100-modal-cycle probes passed. No performance defect inferred from resource size alone. Candidate verdicts await a fresh verifier.

Desktop checks completed 100 image/lightbox cycles and 50 settings round trips. All 30 video reopen cycles also completed. A tag survived full frontend reload. The owned session was stopped, its temporary directory removed and evidence retained. Stage 14 runtime investigation has started.

Foundation peer exchange completed. Canonical candidates DR-001/002 cover conditional storage failures; DR-003 vocabulary finalization and DR-004 retained thumbnail loss come from library-a and await peer challenge. Exact records are in the active run findings.json. Foundation follow-up was briefly deferred by the three-worker capacity limit, then completed.

## Runtime scale and current candidates

The independent adapter probe covered 10,000, 26,000, 100,000 and 1,000,000 logical slots with 30 measured samples per action. Mounted placeholder items stayed at or below 60. This is metadata-only jsdom evidence, separate from media rendering.

The real desktop 2,048-image probe completed cold/warm cache-eviction routes and 30 warmed short actions. It observed at most 91 mounted tiles, median 99.24 ms and p95 136.92 ms including WebDriver overhead. These are not frame-rate measurements. Native WebKit clamps large CSS heights to 33,554,428 pixels; the interaction reviewer is assessing reachable collection size.

The real WebView probe reproduced the suggestion-retry candidate after an injected Worker constructor failure: a pointer click left focus on BODY and made no second worker-construction attempt. Candidate DR-006 remains subject to fresh verification. DR-007 covers prototype-property collisions in tag selection; DR-005 covers a late metadata acknowledgement repopulating stale details. All seven current candidates are provisional. See [findings](../../../raports/20261010T175947Z-solid2/findings.json).

The user explicitly requested a commit during review. This authorizes a checkpoint of these new plan/progress files; application code, existing tests and the six pre-existing edits remain outside this checkpoint. Generated evidence remains ignored under raports/.
