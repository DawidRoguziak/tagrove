---
name: review-team
description: Review the MediaTagger project, a module, a feature, or specified code changes with specialist subagents and independent confirmation. Save evidence and coverage reports under docs/review. Use for ordinary code-review requests; explicit requests for other review skills retain their workflow.
---

# Review team

Act as the review boss. Review defects and justified design risks, excluding cosmetic preferences and unsupported rewrite suggestions. Spawn real specialist agents, then independently verify every distinct candidate. Write English reports unless the user requests another language.

## Boundaries

- Only the boss delegates and writes reports. Workers return findings and coverage in messages. A worker reading this skill follows its assignment without launching a team.
- Keep application source, tests, configuration, documentation outside this invocation's report folder, and existing user changes unchanged. Use temporary synthetic fixtures and build caches for verification. Report documentation drift as a candidate rather than editing the owning page during review.
- Never create worktrees. Never launch the desktop app, app control, or desktop E2E, including aggregate commands that invoke them.
- Reports enter a public repository. Use repository-relative paths, anonymized examples, and concise sanitized test output. Do not copy secrets, private media, local databases, private paths, or raw logs into reports.
- Keep workers' searches within assigned source and documentation paths. Exclude all `docs/review/` output, including the current invocation, so reports cannot bias independent investigation. Pass candidates to confirmers in assignment messages. For directory-wide searches, use an exclusion such as `rg --glob '!docs/review/**'` from the repository root.
- Inherit model and reasoning settings. Run at most three workers concurrently, reduced to available host capacity. Queue bounded assignments when slots are occupied. If delegation is unavailable, record a partial review; do not simulate agents or claim independent confirmation.

## Resolve scope and start the record

Use the request and conversation before asking questions. Ask only if material ambiguity remains, such as two unrelated features sharing the requested name.

| Request | Scope selection |
| --- | --- |
| `review the whole project` | Inventory application, native integrations, tests, executable configuration, and tooling. Use the documentation index to divide these into bounded subsystem assignments and track every included area. Exclude generated, vendored, and build artifacts. |
| `$review-team review src/components/lightbox` | Start with that module, then inspect its callers, dependencies, tests, and contracts needed to judge behavior. Record supporting paths separately from the requested target. |
| `review thumbnail generation` | Follow the documentation index to owning frontend and backend code, then trace the feature through contracts and tests. |
| `review these changes` | Resolve the requested commit range, staged changes, or working diff from context. Record the comparison base. Review the changes in current-file context and include relevant uncommitted edits. |

Read the repository's `AGENTS.md` documentation index, primary topic, and linked contracts needed for the scope. Source, executable configuration, and tests establish behavior; documentation explains intent. Read [report-format.md](references/report-format.md) before writing.

Record the starting commit with `git rev-parse HEAD`, staged and unstaged changes with `git diff --name-status` and `git diff --cached --name-status`, and untracked paths with `git ls-files --others --exclude-standard`. Inspect relevant diffs and current files, including untracked source. If no commit or Git metadata exists, say so and record a file inventory instead. Do not copy a raw diff into public reports. Capture enough file hashes or diff evidence outside the repository to check source preservation at the end without restoring or discarding anyone's edits.

From the repository root, create the invocation folder with:

```bash
python3 .agents/skills/review-team/scripts/init_review.py descriptive-name
```

The helper prints a new `docs/review/YYYY-MM-DD-HHMMSS-name/` path using UTC, adding `-2`, `-3`, and so on on collision. It creates six reports marked partial. Fill `scope.md` before delegating. If Python is unavailable, create the same layout using exclusive directory creation and retry collisions; never reuse an earlier folder. Keep the review partial until all assignments and confirmations are accounted for.

## Assign independent investigations

Spawn only relevant roles. Use multiple bounded assignments per role for large scopes instead of giving each worker the entire repository. Mark an irrelevant role `not applicable` with a reason; unavailable capacity is `blocked`, not `not applicable`.

| Role reference | Assign when |
| --- | --- |
| [Frontend reviewer](references/frontend.md) | React, TypeScript, frontend behavior, accessibility, localization, or frontend tests are in scope. |
| [Backend reviewer](references/backend.md) | Rust, SQLite, filesystem operations, native integrations, or backend tests are in scope. |
| [Architecture reviewer](references/architecture.md) | Ownership, end-to-end flow, integration contracts, or cross-subsystem risks need review. |

Each spawn message must include the repository root, assignment ID, requested and supporting paths, relevant documentation paths, the role reference and report-format reference, baseline, exclusions, and boundaries above. Request inspected paths, candidate findings, and skipped or blocked checks. Use fresh context when supported, such as `fork_turns="none"`, and pass the required context explicitly. Do not share other reviewers' candidate findings during initial investigation. Record the returned agent identity and assignment in `scope.md`.

Use the host's available spawn, message, wait, and completion controls. If a call fails, record the actual limitation, reduce concurrency or queue work as appropriate. Do not invent successful delegation. Keep the user informed and checkpoint the reports after each completed assignment.

## Merge and confirm

Preserve each original candidate ID and source reference. Assign canonical IDs `RT-001`, `RT-002`, and so on, stable for this invocation. Merge candidates only when they describe the same cause, trigger, and impact. Keep the aliases and any differing evidence. Different failure conditions or corrections can warrant separate candidates even at the same line.

Keep an explicit disposition for concrete concerns supplied by the user, even when a reviewer finds a guard that refutes them. If investigation supports a different claim, such as a design risk instead of the alleged runtime defect, record it separately and retain the original claim's verdict. A confirmed risk must not replace an unresolved factual claim.

Spawn a separate fresh confirmation agent for each distinct candidate using [confirmation.md](references/confirmation.md). It must not be any of that candidate's originators. Pass the canonical candidate, all original evidence and references, scope, applicable contracts, and review boundaries. Do not frame reviewer agreement as evidence or supply a desired verdict. The boss's own candidates need the same independent confirmation.

Confirmers return `confirmed`, `rejected`, or `unresolved`, with evidence from executed tests or code and contract analysis. A test failure is evidence only when it establishes the claimed behavior; environment failures are unavailable verification. A missing test runner does not prevent confirmation if the code and contract establish the claim, but uncertainty about a required runtime fact must remain unresolved.

If a confirmer identifies a materially different issue, record it as a new candidate and send it to another fresh confirmer. Queue candidates when necessary. If independent confirmation cannot run, leave the candidate unresolved with the reason. The boss merges verdicts and can request further evidence; the boss does not silently upgrade an unresolved claim. Preserve conflicting evidence and mark the candidate unresolved if it cannot be reconciled.

## Finish reports

Populate all six files using [report-format.md](references/report-format.md). Preserve rejected and unresolved candidates in specialist reports and `verification.md`. Put only confirmed bugs and confirmed design risks in the README's prioritized findings, and list unresolved candidates separately.

Check source preservation against the starting snapshot, cross-report IDs and links, every assignment's coverage status, and every distinct candidate's independent verdict. If concurrent edits changed a relevant baseline, inspect and record them, recheck affected findings, or mark those findings unresolved. Never reset files to enforce preservation.

Mark the invocation `complete` only when every included assignment is complete or justified as not applicable and every candidate has a recorded independent verdict. A complete review can contain unresolved findings if independent investigation could not establish the claim; display that uncertainty. Interrupted assignments, missing workers, or pending confirmation make the review `partial`, with remaining work recorded in both README and scope. A finished review with no confirmed findings is not proof that unreviewed or untested behavior passed.

Return the report-folder link, confirmed counts by priority, unresolved count, and material coverage or verification limits. Do not apply fixes as part of the review.
