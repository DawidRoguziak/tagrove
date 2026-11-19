---
name: gpui-port
description: Run one epic of the Tagrove GPUI port end to end with the saved gpui-port-epic workflow (implement, port tests, check/fix loop, review loop of up to 6 rounds, test:all gate, record), or show the port status. Use only when the user explicitly invokes /gpui-port.
disable-model-invocation: true
argument-hint: "status | <EPIC> | next  [--from implement|tests|stabilize|review|gate] [--commit] [--models role=model[:effort],...] [--allow-dirty] [--force] [--dry-run]"
---

# /gpui-port

Arguments: `$ARGUMENTS`

The plan, the decisions and the agent rules are in
[docs/plans/gpui-port/README.md](../../../docs/plans/gpui-port/README.md). This file is the
coordinator's procedure. Invoking `/gpui-port <EPIC>` is the user's explicit opt-in to run the
saved Workflow `gpui-port-epic` at its full size: one epic uses roughly 25 to 80 agents,
depending on how many review rounds it needs. Do not shrink it to the default workflow size.

Reply in the user's language. Agent prompts and reports stay in English; plan notes are Polish.

All commands run from the repository root. `S=.claude/skills/gpui-port/scripts`.

## Ground rules

- You run scripts and one Workflow. The workflow's agents do the reading, writing and testing.
  Do not edit application code yourself while an epic runs.
- One epic at a time. Never start a second run, a desktop E2E run or an `app:control` session
  while one is active: they share target directories, ports and locks.
- Never create a worktree. Never push. Commit only with `--commit`, after the checks below.
- `--force` and `--allow-dirty` are the user's decisions: pass them only when the user typed them.
- Deferred epics (E20 and later) need a new decision and their own plan first. Say so and stop.
- The repository is public: nothing with a private path or real media name goes into tracked
  files or your reply.

## `/gpui-port status`

`python3 $S/port_state.py status` and show the table, the next runnable epic, and the last five
lines of `docs/plans/gpui-port/NOTES.md`. Then stop.

## `/gpui-port <EPIC>` or `/gpui-port next`

1. **Preflight.** `python3 $S/port_state.py status` (stop on manifest errors). If
   `artifacts/app-control/active.lock` exists, tell the user an app-control session is active and
   stop; never delete the lock.
2. **Arguments.** Pass the user's flags through, apart from `--commit`:

   ```sh
   python3 $S/port_state.py args <EPIC|next> [--from <step>] [--models ...] [--allow-dirty] [--force] > ARGS
   ```

   `ARGS` is a private temp file in your scratch directory. If the command fails, report its
   error text (unmet dependency, dirty tree, deferred epic, protected branch) and stop. The
   command records the review base (`HEAD` of a clean tree) and marks the epic `in progress`.
3. **Run.** `Workflow({name: "gpui-port-epic", args: <the parsed JSON object from ARGS>})`.
   Pass the object, not a string.
4. **Interrupted run.** Resume the same run with
   `Workflow({name: "gpui-port-epic", args: <same object>, resumeFromRunId: <run id>})`. If that
   is not possible, build new args with `--from <first step that did not finish>`; steps are
   `implement`, `tests`, `stabilize`, `review`, `gate`. `--from implement --allow-dirty` continues an
   interrupted implement step and keeps the recorded review base.
5. **Report.** `python3 $S/port_state.py status`, then read the newest entry under
   `## Notatki agentów` in the epic file and, if it exists,
   `docs/plans/gpui-port/reports/<EPIC>-review-open.md`. Tell the user:
   - the status (`done` or `blocked` with the reason);
   - check and gate results, the number of review rounds, confirmed and fixed findings;
   - open findings and where the report is;
   - visual differences left open, deferred scope, new decisions;
   - the next runnable epic.
   Report what the workflow returned. Do not call a blocked epic done, and do not hide a gap.
6. **Blocked.** Do not retry on your own. Name the blocker and the command that continues
   after the user resolved it (usually `/gpui-port <EPIC> --from <step>`).

## `--commit`

Only when the epic ended `done`:

1. `git status --short` and `git diff --stat`: every changed and untracked file belongs to the epic;
   no build output, database, log, screenshot or scratch file.
2. Read the diff for private content (home paths, names, real media), then `bun run privacy:check`.
3. `git add` the epic's files by path and commit on the current feature branch with
   `feat(gpui): <short English summary> (<EPIC>)` and the session's attribution trailer. Never push.

Without `--commit`, leave the tree as it is and remind the user that the next epic needs a clean
tree (the review base is `HEAD`).

## `--dry-run`

`python3 $S/port_state.py args <EPIC> --dry-run > ARGS`, then the Workflow as above. Every agent is
replaced by a trivial one that returns an empty valid result, so the run exercises the control
flow and schemas with about seven small agents. It changes no tracked file and no status.
Expect `status: "done"` in the returned state.

## Models

Roles and defaults are in [references/models.json](references/models.json). `--models
role=model[:effort],...` overrides them for one run, for example
`--models implementer=fable:xhigh,lens=opus`. Hard epics (`tier: hard` in `epics.json`) use
`implementer_hard` and `fixer_hard`.

## Long jobs

Agents never run `bun run test:all` or the comparison harness directly. They go through
`python3 $S/job.py start|wait|tail <EPIC> checks|test-all|compare`, which detaches the job, waits in
steps shorter than the ten-minute command limit, and refuses a display job while another one
or an app-control session is active. Logs: `docs/review/gpui-port/<EPIC>/jobs/` (ignored).

## Maintenance

- `epics.json` is the source of truth. After editing it run `python3 $S/port_state.py validate` and
  `python3 $S/port_state.py render`.
- Script tests: `python3 -m unittest discover -s .claude/skills/gpui-port/scripts`.
