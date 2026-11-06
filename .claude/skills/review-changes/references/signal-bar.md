# Signal bar and finding format

Shared by every review agent and by `/deep-review`. A review is judged by the share of its
findings a maintainer would act on. One false alarm costs more trust than one missed nit.

## What qualifies

Report a candidate only if at least one holds, and you can show it from the code:

1. **Breaks.** A concrete input, event order or state produces wrong results, a crash, data
   loss, a deadlock, a leak, a security hole or a broken user flow. Name that trigger.
2. **Violates a project rule.** It contradicts `CLAUDE.md`, a `.claude/rules/*.md` file or an
   owning `docs/` contract that applies to this file. Quote the rule.
3. **Material cost at real scale.** A performance or resource problem with a named mechanism
   (complexity, I/O count, lock hold, allocation, re-render fan-out) at a scale this app
   supports (up to 1,000,000 query results, large folders, long sessions).

A design risk qualifies only with a concrete failure scenario and its consequence.

## Never report

- Issues the change did not introduce, in the main list. Put them under `pre-existing`, only
  if they are serious (P0/P1) and touched code makes them reachable or worse.
- Anything `tsc`, Biome, rustfmt or clippy `-D warnings` would reject. The tools catch those.
- Style, naming taste, formatting, comment wording, "could be cleaner", alternative designs.
- "Missing tests" alone. A test gap is reportable only when the change's own risk (data
  safety, IPC payload, concurrency) is left with no layer that could catch it.
- Speculation: "might", "could potentially", "if someone later...". Either trace the path
  or drop the claim.
- Code explicitly silenced (lint-ignore, documented `#[allow]`, a comment stating the
  trade-off) unless the stated reason is false.
- Generated files, lockfile contents, vendored code, `raports/`, `artifacts/`, `output/`.
- Behavior the owning doc describes as intended, even if you would design it differently.

## How to reach a verdict

Read before judging. The diff alone causes most false alarms. For each suspicion:

1. Open the whole changed function and its file, not just the hunk.
2. Find the callers and the callee's contract (`grep`/`rg` for the symbol).
3. Look for guards that already prevent the trigger: validation, locks, request IDs,
   generation checks, transactions, early returns, types.
4. Check the tests that cover this path; a passing test that exercises the trigger is
   counterevidence.
5. Write the reasoning, then the verdict. If a guard defeats the claim, drop it.

## Priorities

- **P0**: immediate severe failure with concrete evidence: data loss, corruption, crash on a
  common path, security exposure.
- **P1**: serious failure in a supported workflow.
- **P2**: material impact under specific, reachable conditions.
- **P3**: limited but real and actionable. Cosmetic preferences never qualify.

`confidence` (0–100) is how sure you are that the trigger really produces the impact, after
checking guards. Under 60 means you did not finish the trace. Finish it or leave it out.

## Finding format

Return findings as a JSON array in a fenced `json` block, then a short prose summary.
Return `[]` when nothing qualifies; that is a normal, valuable result.

```json
[
  {
    "id": "rust-1",
    "lens": "review-rust",
    "path": "src-tauri/src/services/scan_service.rs",
    "line": 142,
    "title": "Scan lock held across await while emitting progress",
    "priority": "P1",
    "confidence": 85,
    "origin": "introduced",
    "trigger": "Start a scan, then request a thumbnail clear while progress is emitting",
    "impact": "clear_all_thumbnails waits on thumb_lock behind scan_lock forever; UI hangs",
    "evidence": [
      "scan_service.rs:138-145 `let guard = scan_lock.lock(); ... emit(...).await`",
      "rules/backend-core.md: 'Take workflow locks inside spawn_blocking, never across an .await'",
      "Callers checked: commands/scan.rs:57 (no other guard)"
    ],
    "counterevidence": "None found; with_scan_and_thumb_lock is not used here",
    "fix": "Move the locked section into spawn_blocking and emit after the guard drops"
  }
]
```

- `origin`: `introduced` (by this change) or `pre-existing`.
- `line`: a line in the current file (new side of the diff), positive.
- `evidence`: quoted code with `path:line`, quoted rules, callers and guards you checked.
- `fix`: the smallest correction that removes the trigger. No rewrites.

After the array, add 2–5 lines: what you inspected beyond the diff, what you could not check,
and any cross-lens question for another specialist (for example, "frontend: does the caller
discard stale results?").
