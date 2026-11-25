---
name: audit
description: Staged, resumable, multi-agent audit of the whole Tagrove project - static finders with one refuter and one judge per finding, then a real-app QA click-through on a sandboxed copy of a large dataset, then a report. `/audit` runs every stage to the end on its own; state lives on disk under docs/review/<run>/. Use only when the user explicitly invokes /audit.
disable-model-invocation: true
argument-hint: "[--dataset <path>] [--new] [--name x] [--models role=model[:effort],...]  |  <stage> (init, static, static-verify, qa-prep, qa-run [S1-S4], qa-verify, report, status)"
---

# /audit

The process, roles, models, safety model and cost are described in
[docs/development/audit/README.md](../../../docs/development/audit/README.md). This file is the
coordinator's procedure. Invoking `/audit` is the user's explicit opt-in to run the saved audit
Workflows at full scale (about 95 finder agents in the static stage, more in verification);
do not shrink the stages to the default workflow size.

## Modes

- **`/audit [--dataset <path>] [--new] [--name x] [--models ...]` — automatic (default).** Run
  every stage to the end without asking the user anything (section "Automatic run"). The user
  expects to come back to a finished `report.md`.
- **`/audit <stage> ...` — one stage.** Run only that stage (section "One stage") and stop.

## Ground rules

- You run scripts and one Workflow per step; agents do the reading and clicking. Read only
  counters, `STATUS.md` and script output, never raw agent results.
- Never edit application files, never create a worktree, never commit. Everything lands under
  the ignored `docs/review/<run>/`.
- Never print, paste or commit the dataset path. Refer to it as `$AUDIT_DATASET`.
- **`halted` is the only hard stop.** A script sets it when the dataset changed, a session did
  not stop cleanly, or a process wrote outside the copy. Stop everything, report the reason
  from `STATUS.md`, and tell the user that only they can resume with
  `python3 .claude/skills/audit/scripts/clear_halt.py --confirm-inspected` after inspecting the
  run. Never run that script yourself.
- Everything else is handled without the user: failed items are retried, a stage that keeps
  failing is recorded as a gap, and the run continues to the report.
- `--models role=model[:effort],...` overrides roles from
  [references/models.json](references/models.json); pass it to every `pending` call.

All commands run from the repository root. `S=.claude/skills/audit/scripts`.
`ARGS` is a private temp file (for example `$CLAUDE_JOB_DIR/tmp/audit-args.json`).

## Stage step (used by both modes)

To run stage `<stage>` once:

1. `python3 $S/audit_state.py status`; stop the whole run if halted.
2. For `qa-run` and `qa-verify`: `python3 $S/qa_session.py reap`.
3. `python3 $S/audit_state.py pending --ids <stage> [--block B] [--models ...] > ARGS`. If the
   command fails or `items` is empty, the step is done.
4. `Workflow({name: "audit-<stage>", args: <parsed JSON object from ARGS>})`.
5. Follow-ups from the table, then `audit_state.py status`.

| Stage | Before the workflow | After the workflow |
| --- | --- | --- |
| `static` | — | `assign-ids --kind static`, `dup-index` |
| `static-verify` | — | `dup-index` |
| `qa-prep` | — | — |
| `qa-run` | `qa_session.py reap` | `qa_session.py reap` |
| `qa-verify` | `qa_session.py reap` | `assign-ids --kind qa`, `qa_session.py reap` |
| `report` | `coverage.py`, `dup-index` | — |

**Retry:** after a step, run `pending` again. While items remain, repeat the step, at most 3
times per stage in total. `qa-verify` needs at least two steps (dedup first, then the
records), so it gets 4. Items still pending after that are gaps; continue with the next stage.

## Automatic run

1. **Run selection.** If `--new` was given, or `audit_state.py status` fails (no run), or the
   latest run already has `report: true`: `python3 $S/audit_init.py --dataset <path>
   [--name x] [--models ...]`. A missing `--dataset` here is the only reason to ask the user
   anything; ask once and continue. Otherwise resume the latest run where it stopped (every
   step is idempotent).
2. **Static:** stage steps `static`, then `static-verify`.
3. **QA gate.** QA runs only if all of these hold; otherwise skip to step 5 and record the
   reason in your final message:
   - `context/tools.json` has no missing required tools, and `run.json.hooks_missing` is empty;
   - `run.json.smoke.bwrap_gtk` and `run.json.smoke.reflink` are `pass`;
   - the hook smoke passes: `python3 $S/audit_state.py hook-check --prepare`, then
     `Workflow({name: "audit-hook-smoke", args: {run, models}})` (take `run` and `models` from
     any `pending` output), then `python3 $S/audit_state.py hook-check --evaluate` must print
     `"status": "pass"`. If it fails, do not retry; QA is unsafe and is skipped. If
     `run.json.hook_smoke.status` is already `pass`, skip this.
4. **QA:** stage steps `qa-prep`, `qa-run` (all blocks), `qa-verify`.
5. **Report:** stage step `report`.
6. Final message: counters from `STATUS.md`, the most important confirmed findings (P0/P1),
   gaps (stages or items that kept failing, skipped QA and why), and the path of `report.md`.

## One stage

`/audit init --dataset <path> [--name x] [--models ...]`: run `audit_init.py` and report its
JSON. `/audit status`: run `audit_state.py status` and show `STATUS.md`. Any other stage: run the
stage step with the retry rule. For `qa-run`/`qa-verify` the hook smoke from the automatic run
must have passed first (`pending` refuses otherwise); run the gate in step 3 if it has not.

## References

- [finding-format.md](references/finding-format.md): static finding, QA bug, record schema,
  verdicts.
- [safety.md](references/safety.md): dataset copy, sandbox, hooks, guards, session lifecycle.
- [qa-driving.md](references/qa-driving.md): controller commands, oracles, traps.
- [known-sources.md](references/known-sources.md): what counts as `known`.
- [batches.json](references/batches.json), [models.json](references/models.json),
  [qa-blocks.json](references/qa-blocks.json), [recon-candidates.json](references/recon-candidates.json).
- Scenario catalog: [docs/development/audit/qa-scenarios.md](../../../docs/development/audit/qa-scenarios.md).
