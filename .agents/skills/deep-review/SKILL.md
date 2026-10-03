---
name: deep-review
description: Explicit multi-agent review of MediaTagger changes, files, modules, or the whole project. Independently investigate, exchange evidence, and verify candidates; save local reports under raports/. Use only when explicitly requested as deep-review.
---

# Deep review

Review behavior and substantiated security, data-safety, performance, UX, architecture,
and maintainability risks. Propose corrections without applying them. Reports default
to English. Invoke as `$deep-review current changes`, `$deep-review src/components/lightbox`,
or `$deep-review whole project`. With no target, review staged, unstaged, and relevant
untracked changes against `HEAD`. An empty diff produces a no-changes report.

## Boundaries

Only the coordinator spawns agents and owns shared indexes and the final report.
Workers own `agents/<assignment>/` and their assigned verification files. Never create
worktrees. Preserve application files, existing changes, and historical reports.
Keep all generated review artifacts under this run's ignored repository-root `raports/`
folder, including temporary probes. Existing test tools may retain their already-ignored
artifacts elsewhere; link or copy sanitized evidence into the run. Never force-add reports.
Do not copy secrets, private media, databases, or private logs into evidence.

Initial reviewers must not read peer notes or previous reports. Exclude `raports/` and
`docs/review/` from broad source searches. Read only explicitly assigned synthetic fixtures,
coordinator-provided context and owned output there. Open peer evidence only after the coordinator starts collaboration.
A delegated worker follows its assignment, without launching another review team.

## Start and map

Read [the record contract](references/records.md). From the repository root run:

```sh
python3 .agents/skills/deep-review/scripts/init_review.py
# Explicit scope, with the requested source roots:
python3 .agents/skills/deep-review/scripts/init_review.py lightbox --path src/components/lightbox
```

The initializer creates a collision-safe UTC directory and captures Git status, `HEAD`,
and file hashes. It does not decide relevance or finish a review. Record the user's
request, comparison base, exclusions and supporting paths in `run.json`. For a requested
commit range, capture that diff explicitly and use `--path` for its files. For a whole
project review use `--path .`, then inventory subsystems and queue bounded batches.
Generated and vendored files are excluded by Git's ignore rules. If Git or `HEAD` is
unavailable, record the limitation and an explicit source inventory; do not claim a clean diff.

Route through the documentation index in repository [AGENTS.md](../../../AGENTS.md).
Read the primary topic and linked contracts needed for this scope. Source, executable
configuration and tests establish behavior. Record documentation drift as a candidate.
Write `context.md` with project facts, dependency map, callers, contracts, test entry
points and exclusions. Add supporting source hashes using the helper's `snapshot`
function before assigning those files. Divide substantive scope into coverage areas.
Every area needs at least two independent reviewer identities and different perspectives.
Discovery alone and subsequent verification do not replace those perspectives.

## Models and assignment

Read [roles and runtime checks](references/roles-and-runtime.md). Check model availability
from the live tool schema or model-list capability on every run and record the source.
Use native collaboration tools with `fork_turns="none"`, explicit `model` and
`reasoning_effort="high"`. Check live capacity; run at most three workers concurrently,
reduced to available slots, and queue the rest. Only the coordinator spawns.

| Assignment | Preferred model |
| --- | --- |
| Code mapping, documentation lookup, test discovery | `gpt-6-luna` |
| Frontend behavior, accessibility, localization | `gpt-6.1-sol` |
| Rust, SQLite, IPC, filesystem correctness | `gpt-6.1-sol` |
| Architecture, maintainability, performance, test adequacy | `gpt-6.1-sol` |
| Runtime reproduction, ordinary independent verification | `gpt-6.1-sol` |
| Difficult concurrency, data-loss/security risks, disputed conclusions | `gpt-6-astra` |

Luna falls back to Sol. Sol falls back to `gpt-6-sol`, then Astra. Astra falls back to
Sol, using the same Sol fallback chain without retrying an already failed model.
Record requested and selected models, reasoning, returned identities, failure reasons,
and observed model metadata when exposed. A requested model is not proof of the model
that ran. Use null for unavailable observed metadata. Missing delegation means partial
coverage, never simulated agents. The coordinator keeps the invoking session's model;
recommend Sol for that session, but do not claim to change it.

This routing is a project workflow choice. Official [model guidance](https://developers.openai.com/api/docs/guides/latest-model)
and [subagent guidance](https://learn.chatgpt.com/docs/agent-configuration/subagents)
support Luna for narrow work and Sol for demanding work. Reserving Astra and requiring
high reasoning here are local choices. The independent investigation and separate
validation pattern adapts [Anthropic's review workflow](https://github.com/anthropics/claude-code/blob/main/plugins/code-review/commands/code-review.md).
Do not import that workflow's publishing steps or exclusions of pre-existing issues.

## Independent review, then collaboration

Give each fresh worker its assignment ID, role, repository root, run path, source
baseline, bounded requested and supporting paths, applicable documentation, ownership,
and this skill's boundaries. Include the record contract and role reference. Do not
supply other reviewers' conclusions. Ask for actual inspected paths, commands, limitations,
and independent findings in its own files. Save even a zero-finding investigation.

Wait until the relevant independent notes and findings are saved before opening a
collaboration round. Preserve those originals. Send messages pointing to written evidence,
ask specific questions, and let reviewers read relevant peer notes, investigate cross-module
effects, and save responses in their own `collaboration.md`. Track each handoff in
`run.json` with sender, recipient, evidence, question, response and status. Send replies
through native messages. Keep unanswered or blocked questions visible.

Merge only candidates with the same cause, trigger and impact. Give each distinct candidate
a stable `DR-NNN` ID. Preserve every original ID, author and evidence reference in
`findings.json`, including duplicate mappings. Explicit user concerns require a disposition,
even when guards refute them. Do not replace an unproven defect with a different design risk.

Assign every distinct candidate to a fresh verifier who did not originate any merged
claim. Supply the claim and all evidence, including counterevidence, without a desired
verdict. The verifier reads current source, callers and contracts and tries to falsify
it. Agreement is insufficient. Run focused checks where a material runtime fact needs
proof. Save `confirmed`, `rejected`, or `unresolved` with evidence and verified hashes.
A verifier's newly discovered issue needs a different verifier. Route difficult disputes
to Astra; preserve disagreement if evidence cannot settle it.

## Finish

Check assignment coverage, handoffs, aliases and independent verdicts. Follow up only to
resolve a specific gap. Record blocked work as partial. Recheck changed source affecting
a finding or mark it stale and unresolved; never reset concurrent edits. Include supporting
contracts in each finding's verified hashes, not just the highlighted line.

Write `report.md` with scope and status, prioritized confirmed findings and proportionate
corrections, introduced versus pre-existing classification, rejected count, unresolved
concerns, coverage, verification results and material gaps. Summarize commands and observed
results, inspected screenshots, persistence checks and cleanup when applicable.

```sh
python3 .agents/skills/deep-review/scripts/validate_review.py raports/<run>
```

Resolve validation errors. A valid partial report is still partial. Finish only when every
assignment and handoff is accounted for and all candidates have independent verdicts, or
explicitly report remaining work as partial. The no-changes case needs no workers. Return
a link to `report.md`, confirmed counts by priority, rejected/unresolved counts and material
verification gaps. Do not claim that an untested area passed.
