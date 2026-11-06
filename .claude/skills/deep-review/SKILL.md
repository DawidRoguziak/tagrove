---
name: deep-review
description: Explicit, saved multi-agent audit of Tagrove changes, paths, or the whole project. Two independent specialist lenses per area, an independent verifier per finding, optional test and runtime proof, and validated records under raports/. Use only when explicitly requested as deep-review.
disable-model-invocation: true
argument-hint: "[current changes | --range <A..B> | --base <rev> | <path>... | whole project]"
---

# Deep review

This is the heavy variant of `/review-changes`. It uses the same specialist agents
(`.claude/agents/review-*.md`), the same signal bar
([signal-bar.md](../review-changes/references/signal-bar.md)) and the same verifier. On top of
those it adds a whole-path or whole-project scope, two independent lenses on every file,
optional runtime proof, and records that a script can validate. It proposes corrections and
never applies them. Reports are written in English.

Examples: `/deep-review` (branch plus uncommitted work), `/deep-review --base origin/main`,
`/deep-review src/features/lightbox`, `/deep-review whole project`.

## Boundaries

- Only the coordinator spawns agents and writes records. Specialists are read-only, so you
  save their output yourself.
- Never create worktrees. Never edit application files or reset existing changes. Keep every
  artifact, including temporary probes, inside this run's directory under the ignored
  `raports/`. Never force-add reports.
- Never copy secrets, private media, databases or private logs into evidence.
- Fresh agents get the brief and their assignment, and never your conclusions. Don't use
  `fork`. Exclude `raports/` and `docs/review/` from searches during the independent phase.

## 1. Initialize and scope

Read [the record contract](references/records.md). Then, from the repository root:

```sh
python3 .claude/skills/deep-review/scripts/init_review.py                          # branch + uncommitted
python3 .claude/skills/deep-review/scripts/init_review.py --base origin/main       # explicit base
python3 .claude/skills/deep-review/scripts/init_review.py release --range v1.2..HEAD # committed range only
python3 .claude/skills/deep-review/scripts/init_review.py lightbox --path src/features/lightbox
python3 .claude/skills/deep-review/scripts/init_review.py whole-project --path .
```

The initializer resolves the change set with `review-changes/scripts/collect_scope.py`, so the
default is `merge-base(main)..HEAD` plus the working tree. It creates a collision-safe run
directory and records the baseline hashes.

- Record the user's request and any exclusions in `run.json`.
- If `no_changes` is true, write the no-changes report and stop. No agents are needed.
- For a whole project, inventory the subsystems from the CLAUDE.md layout and queue bounded
  batches of at most about 40 files per assignment.

## 2. Context and areas

1. Route through the CLAUDE.md documentation index and read the owning pages for the scope.
2. Run `collect_scope.py` (or read its output) to get the matched `.claude/rules` and docs.
3. Write `context.md` with:
   - the intent;
   - the dependency map (callers and contracts);
   - the rules and docs that apply;
   - the test entry points;
   - the exclusions.
4. Hash every supporting file you assign, with `init_review.snapshot`, and add it to
   `baseline.hashes`.
5. Split the scope into coverage areas. Every area needs **two completed review assignments
   with different lenses**:

| Area content | Lenses (pick at least 2) |
| --- | --- |
| Rust, SQLite, IPC, filesystem | `review-rust` + `review-conventions` (+ `review-performance` on hot paths) |
| Frontend, models, a11y, i18n | `review-frontend` + `review-conventions` (+ `review-performance` for gallery/query) |
| Hot paths (gallery, query, thumbnails, scan, db) | the domain lens + `review-performance` |
| Docs, tooling, packaging | `review-conventions` + the domain lens the change affects |

## 3. Independent review

Launch the assignments with the Agent tool, using `subagent_type: "<lens>"`. Launch
independent ones in one message. The skill explicitly requests `effort: "high"` (the default
in the agent files).

Each prompt contains:
- the repository root, the run path and the baseline `head`/`comparison_base`;
- the diff command, or for path mode the file list;
- the assignment's files, rules and docs;
- the user's concerns;
- "read-only; return the signal-bar JSON and summary".

For every finished agent, save its JSON array verbatim to `agents/<id>/findings.json` and
its summary to `agents/<id>/notes.md`. Then record the returned agent ID, lens, model, effort
and status in `run.json`. Save the original files before any merging, and never modify them
afterwards.

Collaboration is optional. When an agent raises a cross-lens question that bears on a
candidate, continue the other agent with `SendMessage` and record the exchange in `handoffs`.
Save the reply to `agents/<recipient>/collaboration.md`. Unanswered handoffs keep the run
partial.

## 4. Merge

Give each distinct candidate a stable `DR-NNN` ID in `findings.json`.

- Merge originals only when they share a cause, a trigger and an impact.
- Keep every original ID, every originator and all evidence, and map duplicates.
- Never swap an unproven defect for a different design risk.
- Explicit user concerns need a disposition even when guards refute them; record them as
  intake originals.

## 5. Verify

For each candidate, launch a fresh `review-verifier` that did not originate any of its
merged claims. Pass the candidate with its evidence and counterevidence, and don't say which
verdict you expect.

- Use `effort: "max"` for P0/P1, for disputes and for user concerns.
- Save the result as `verification/DR-NNN.json`.
- A verifier's `new_issue` becomes a new candidate and goes to a different verifier.

**Runtime proof** applies when a verdict depends on a runtime fact (tests, E2E and the
desktop app are allowed in this skill). Assign one general-purpose runtime owner and follow
[roles-and-runtime.md](references/roles-and-runtime.md). Pick the lowest test layer that
proves the claim, and record the commands and observations in the verification file
(`basis: "executed"` or `"both"`).

## 6. Finish

1. Recheck hashes. For changed sources, re-verify the finding or mark it `stale` and
   `uncertain`.
2. Write `report.md` covering:
   - the scope and status;
   - confirmed findings grouped by priority, each with the proportionate correction and
     whether it was introduced or pre-existing;
   - uncertain items, each with the missing fact;
   - the rejected count;
   - coverage by area and lens;
   - the commands and their results;
   - gaps and cleanup.
3. Validate the run until it is clean:

```sh
python3 .claude/skills/deep-review/scripts/validate_review.py raports/<run>
```

A valid partial report is still partial; never claim that an untested area passed. Return
the link to `report.md`, the confirmed counts by priority, the uncertain and rejected counts,
and the material gaps.
