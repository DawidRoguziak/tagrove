# /audit finding format

Every result is JSON validated by `audit_state.py put`. Invalid input is refused with a reason;
fix it and retry once. All text is English. Paths are repo-relative and never contain a home
directory or the dataset path.

## Audit mode (overrides parts of the signal bar)

The bar, exclusions and priorities of
[signal-bar.md](../../review-changes/references/signal-bar.md) apply, with these changes,
because `/audit` reviews the whole project rather than a change:

- every finding has `origin: "pre-existing"` (finders), `"recon"` (planning reconnaissance,
  added by the coordinator after the finders) or `"tool"` (a failing check from the tool
  runner);
- P2 and P3 findings belong in the main list; the "only if serious and touched" limit for
  pre-existing issues does not apply;
- everything else holds: a concrete trigger, a traced mechanism with `path:line`, checked
  guards, no style, no linter-class issues, no speculation.

Finders work independently: do not read `docs/review/`, `raports/` or other agents' output.

## Static finding (finders)

```json
{
  "title": "Thumbnail render keeps the scan lock across the ffmpeg wait",
  "file": "src-tauri/crates/media_tagger_core/src/services/thumb_service.rs",
  "line": 412,
  "end_line": 440,
  "category": "stability",
  "priority": "P2",
  "origin": "pre-existing",
  "trigger": "Render all while a rescan is queued on a folder with long videos",
  "mechanism": "with_scan_lock is held while waiting for ffmpeg without a timeout (...)",
  "impact": "Rescan waits for the slowest video; the Settings loader never clears",
  "evidence": ["src-tauri/crates/media_tagger_core/src/services/thumb_service.rs:412 quoted code", "callers/guards checked"],
  "fix_hint": "optional, one sentence",
  "confidence": 80
}
```

`category`: `bug`, `perf`, `stability`, `convention`, `data-safety`. `priority`: `P0`–`P3`.
`evidence` needs at least one entry.

## Raw finder output (`put raw`, one per batch and lens)

```json
{"batch": "B3a", "lens": "rust", "findings": [ ... ],
 "leads": [{"title": "unfinished suspicion", "file": "path", "line": 1, "note": "what to check"}],
 "files_reviewed": ["src-tauri/crates/media_tagger_core/src/db.rs", "..."]}
```

Store it even when `findings` is empty, so the item is not run again. Leads are not verified;
the report lists them.

## Static dedup (`put dedup`, one per batch)

References are `<batch>:<lens>#<index>` into the raw files.
`{"batch": "B3a", "groups": [{"keep": "B3a:rust#0", "merged": ["B3a:stability#1"], "reason": "same root cause"}]}`

## Refuter result (`put refutation`)

Static (agent `review-verifier`):

```json
{"id": "S-1a2b3c4d", "verdict": "confirmed|rejected|uncertain", "priority": "P2",
 "reasoning": "...", "evidence": ["..."], "counterevidence": ["..."],
 "missing_fact": "required when uncertain", "new_issue": null}
```

QA (agent `qa-refuter`) adds `"grounds": "none|intended|known|artifact|not-reproducible"` and
`"reproduced": "0/2"`. `put` prints `judge_role`; return it to the workflow.

## Judgment (`put judgment`, agent `audit-judge`)

```json
{"id": "S-1a2b3c4d", "verdict": "confirmed", "priority": "P2", "category": "stability",
 "reasoning": "...", "evidence": ["..."],
 "duplicate_of": null, "known_source": null, "missing_fact": null}
```

| Kind | Verdicts |
| --- | --- |
| static | `confirmed`, `rejected`, `duplicate` (needs `duplicate_of`), `known` (needs `known_source`), `unproven` (needs `missing_fact`; final for statics, feeds the QA planner) |
| QA | `confirmed`, `rejected`, `not-reproducible`, `duplicate`, `known` |

## QA bug (inside `put qa-suite`, and triage `new_candidates`)

```json
{"title": "Shift-click range across unloaded pages selects one item too many",
 "area": "bulk-selection",
 "steps": ["Enable bulk actions", "Click tile 3", "Shift-click tile 12000 after scrolling with End"],
 "expected": "data-selected-count=11998", "expected_ref": "docs/subsystems/search-tags-and-media-groups.md#...",
 "actual": "data-selected-count=11999",
 "evidence": ["artifacts/app-control/<id>/screen-0042.png", "actions.jsonl 12:03:11 modclick Shift ...",
              "console.jsonl 12:03:12 warn ..."],
 "frequency": "2/2", "priority": "P2", "category": "bug", "suspect": "src/features/gallery/createSelection.ts:210"}
```

Triage candidates add `"suite": "QA-06"`.

## QA suite report (`put qa-suite`)

```json
{"block": "S1", "suite": "QA-06", "status": "completed|partial|blocked|aborted",
 "session": "<32 hex session ID>", "summary": "...",
 "scenarios_run": ["QA-06.1"], "scenarios_skipped": ["QA-06.9: reason"], "bugs": [ ... ]}
```

## QA plan (`put plan`, planner)

```json
{"suites": [{"id": "QA-01", "block": "S1", "title": "First scan from the UI",
             "scenario_ref": "docs/development/audit/qa-scenarios.md#qa-01-first-scan-from-the-ui", "destructive": false,
             "perf_build": false, "focus": ["optional extra checks"], "targets": []},
            {"id": "QA-T1", "block": "S2", "title": "Reproduce S-1a2b3c4d", "scenario_ref": "records/S-1a2b3c4d.json",
             "destructive": true, "targets": ["S-1a2b3c4d"]}]}
```

Suites listed as destructive or `perf_build` in `qa-blocks.json` must keep those flags.

## Record (written by the scripts)

`records/<S|Q>-<8 hex>.json`: `{schema, id, kind: static|qa, state: candidate|refuted|judged,
finding, sources, merged_from, batch, refutation, judgment, history}`. The ID is a hash of
`file:line:normalized title` (static) or `block:suite:normalized title` (QA), so re-running a
stage never duplicates records. `refutation` and `judgment` are each written exactly once.
