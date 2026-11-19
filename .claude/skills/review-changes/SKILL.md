---
name: review-changes
description: High-signal, read-only review of Tagrove changes. Specialist agents (Rust, frontend, performance, project rules) review independently, a fresh verifier tries to falsify each finding, and only confirmed issues are reported. Use when the user asks to review changes, a branch, a commit range, staged work, or a path's changes. Reports in the terminal; writes a file only with --save.
argument-hint: "[<base> | <A..B> | <path>...] [--staged] [--base <rev>] [--save]"
---

# Review changes

Arguments: `$ARGUMENTS`

Find the defects in this change that a maintainer would act on, and nothing else. This
review is **read-only**: no edits, builds, tests or app launches. The single exception is
the report file under `--save`. Use `/deep-review` instead when the user wants a saved,
multi-perspective audit with test execution.

Reply in the user's language. Agent prompts and the saved report stay in English.

Copy this checklist and track it:

```
- [ ] 1 Scope    - [ ] 2 Triage   - [ ] 3 Brief
- [ ] 4 Review   - [ ] 5 Filter   - [ ] 6 Verify   - [ ] 7 Report
```

## 1. Scope

From the repository root, pass the arguments through unchanged, apart from `--save`:

```sh
python3 .claude/skills/review-changes/scripts/collect_scope.py [<A..B> | <path>...] [--staged] [--base <rev>]
```

- The default is `merge-base(main)..HEAD` plus staged, unstaged and untracked work. On
  `main`, the default is uncommitted work only.
- A single bare revision such as `/review-changes origin/main` means `--base origin/main`.
- If `empty` is true, report "no changes in scope" with the base and mode, and stop. When a
  path filter emptied the scope, suggest `/deep-review <path>` for a whole-module audit.
- Show the scope to the user in one line: mode, base..target, the number of files and lines,
  and the commit subjects.

## 2. Triage

Use `suggested_lenses` from the scope JSON as the default:

| Lens agent | Runs when |
| --- | --- |
| `review-conventions` | always (unless solo) |
| `review-rust` | anything under `src-tauri/` or `src-gpui/` changed |
| `review-frontend` | `src/` or `e2e/` changed |
| `review-gpui` | anything under `src-gpui/` changed (the GPUI port workspace) |
| `review-performance` | a hot path changed, or more than 300 app source lines |

You may add a lens when the diff warrants it (for example, a tooling script that edits
locale files). Write down why. An empty list means solo mode: a small docs- or config-only
change that matches no rule. Review it yourself against `references/signal-bar.md`, then
skip to step 7.

For more than about 40 files or 3,000 changed lines, split each lens across 2 agents by
directory, so that no agent skims. Lockfiles and locale JSON never count toward the split.

## 3. Brief

Write one neutral brief that every agent receives verbatim. Give facts and leave out
hypotheses:

```
Repository root: <root>. Read-only review; do not edit, build or run tests.
Intent: <one or two sentences from commit subjects, branch name and user's words>
Scope: <mode>, base <sha12>, target <target>; <n> files.
Diff: `<diff_command>` (plus untracked: <list or none>)
Your files: <all, or the subset for this split>
Rules to read: <scope.rules[*].rule>   Docs to read: <scope.docs>
User concerns: <verbatim, or none>
```

When the user named a concern, list it under "User concerns" and make sure it gets a
verdict in the report, even if it turns out to be harmless.

## 4. Independent review

Launch every selected lens in **one message** with the Agent tool, using
`subagent_type: "<lens>"` and the brief as the prompt. The agents don't see each other's
work. Don't use `fork` and don't add your own suspicions to the prompts. Fresh agents with no
anchoring are the point.

## 5. Merge and filter

Parse each agent's JSON array. Then:

1. **Drop** candidates with confidence below 60, those that hit an exclusion in
   `references/signal-bar.md`, and those with no concrete trigger.
2. **Merge** candidates only when they share a cause, trigger and impact. Keep every
   original ID and the evidence from each source.
3. **Separate** `pre-existing` candidates. They are verified like the others but reported in
   their own section, and only at P0/P1.
4. Collect the agents' cross-lens questions. If one points to a concrete unchecked path,
   turn it into a candidate. Otherwise list it under coverage gaps.

If nothing survives, go to step 7 and report a clean result. Don't go looking for something
to say.

## 6. Verify

For each surviving candidate, launch one `review-verifier`, all in **one message**. Its
prompt contains the brief, then the candidate JSON as written, including its
counterevidence, and nothing about which verdict you expect.

- Use `effort: "max"` for P0/P1 candidates, for candidates that two lenses disagreed on, and
  for user concerns. This skill explicitly requests that effort.
- With more than 8 candidates, keep one verifier per P0/P1 candidate and group the rest into
  verifiers of up to 3 candidates from the same file or subsystem.
- A verifier's `new_issue` goes to a second, fresh verifier, at most once per issue. Never
  report a `new_issue` unverified.

Use the verifier's priority and origin when it corrected them.

## 7. Report

Print in the terminal, in the user's language and in this order:

```
## Review: <branch or range> (<n> files, base <sha12>)

### Findings (<count>)
**P1** `path:line` Title
  Trigger → impact, in one or two sentences.
  Fix: smallest correction.   [lens → verified]

### Pre-existing, made reachable by this change   (omit when empty)
### Needs a runtime check                        (verdict uncertain; name the missing fact)
### Coverage
Lenses: … · Rejected: n (one line each: id, reason) · Not checked: native rendering, tests not run, …
Suggested checks: `bun run typecheck`, `bun run lint`, `<targeted test command>`
```

- Sort by priority. Keep each finding to 4 lines or fewer, and put the long evidence behind
  `path:line` references.
- The report contains only verdicts of `confirmed`. Nothing else appears as a finding.
- In the coverage line, say what was not reviewed. Don't call an unverified area clean.
- Suggested checks name the lowest test layer for each risk, using the CLAUDE.md
  "Verification" list and `docs/development/testing.md`. You don't run them.

**With `--save`:** write the same report in English to
`raports/<UTC yyyymmddTHHMMSSZ>-review-<branch-or-range-slug>.md`, adding the brief and each
verifier's evidence for the confirmed findings. Check first that `git check-ignore -q raports/x`
succeeds. This is the only file the review creates.

## Boundaries

- Never edit source, stage, commit, or create worktrees. Never run builds, tests, E2E,
  `bun install`, or `cargo`.
- Never paste secrets, private paths or media names from the diff into the report. Refer to
  them by `path:line`.
- If an agent fails or hits `maxTurns`, rerun it once with a narrower file subset. If it
  fails again, record the gap under Coverage. Never simulate an agent's result.
