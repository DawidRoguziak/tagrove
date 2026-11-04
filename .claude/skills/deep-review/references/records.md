# Record contract

The coordinator owns `run.json`, `context.md`, `findings.json` and `report.md`. Each worker
owns `agents/<assignment>/notes.md`, `findings.json`, `collaboration.md` and evidence files.
Verifiers also own their assigned `verification/<candidate>.json`. References in JSON
are paths relative to the run directory; they must resolve to files inside that directory.
Source locations and hash keys are repository-relative. Never overwrite another worker's
notes. Independent notes and original findings remain unchanged after collaboration;
write new collaboration claims to a separate assignment with their own original IDs.

The initializer supplies the run skeleton and baseline. Fill these records as work proceeds.
Validation accepts partial reports with explicit limitations, but never counts them as complete.

## Run and assignments

`run.json` uses schema version 1. `scope` records the request, mode `changes` or `paths`,
targets, included files, supporting files, exclusions with reasons, and `no_changes`.
The baseline contains `head`, `comparison_base`, staged/unstaged/untracked path lists and
`hashes`. Hashes are SHA-256 strings, `symlink:<hash>` for link identities, or null for
missing/deleted files. Add hashes for supporting files before investigation. The helper
exports `snapshot(root: Path, names: list[str])` for this purpose. Hash submodule source
files explicitly rather than passing the Git submodule directory as a file.

`no_changes: true` also requires an empty Git diff between the recorded `comparison_base`
and `head`. The validator resolves both endpoints as commits and compares their trees,
even when the checkout has since moved. Different commits with identical trees are valid.
A nonempty diff or an unavailable endpoint rejects the claim. If the range cannot be
verified, use `status: "partial"`, set `no_changes: false`, and describe the missing
comparison in `limitations`.

`coordinator` contains `agent_id` and `observed_model`, null if unavailable.
`model_availability` records `source`, `available` model names and `substitutions`, each
with requested/selected model and reason. Preserve runtime metadata as observed; never
infer an observed model from the request. Record spawn failures in substitutions/limitations.
`runtime_owner` is an agent identity or null if no runtime work is assigned.
`source_changes` lists files whose current hash differs from the baseline. Recheck affected
findings or mark them stale and unresolved. `limitations` lists gaps, including missing tools.

Each assignment has:

```json
{
  "id": "frontend-a", "role": "review", "agent_id": "returned runtime identity",
  "perspective": "frontend lifecycle", "areas": ["lightbox"], "status": "complete",
  "requested_model": "opus", "selected_model": "opus",
  "reasoning_effort": "high", "observed_model": null,
  "notes": "agents/frontend-a/notes.md",
  "findings": "agents/frontend-a/findings.json",
  "collaboration": "agents/frontend-a/collaboration.md", "limitations": []
}
```

Roles are `discovery`, `review`, `verification`, `runtime` or coordinator `intake`. Status is `pending`, `running`,
`complete`, `blocked` or `not-applicable`. Non-complete assignments need a reason in
`limitations`. A complete run permits only complete or justified not-applicable assignments.
Coordinator intake uses the invoking model and reasoning, with null metadata when unexposed.
Failed spawns may have null selected model and agent identity. Each complete assignment saves all three files, even when findings is `[]` or collaboration
notes explain why no peer question applied. Notes record inspected paths, actual commands,
results and skipped work. Save independent files before reading peers.

`coverage` entries contain `id`, `paths` as expanded file paths, and `assignments` as IDs.
Every included file must appear, and each area needs two completed review assignments from
different agent identities and perspectives. Supporting files need hashes but not exhaustive
review coverage. Inventory all subsystems for a whole-project review; batch by area.

`handoffs` entries contain `id`, `sender` and `recipient` identities, `question`, `evidence`
file, `response` file or null, and `status` of `pending`, `answered` or `blocked`. Each
substantive run needs a collaboration round. Complete runs have no unanswered handoffs.
Empty-diff reports need no assignments, coverage or handoffs; describe that no review ran.

## Candidates and aliases

Each worker's `findings.json` is a list of original candidates. Every entry contains `id`,
`title`, `location` with source `path` and positive `line`, `trigger`, `impact`, `priority`,
`evidence` as file references, `counterevidence` as prose entries, `suggested_correction`,
`classification` of `defect` or `design-risk`, and `change_origin` of `introduced`,
`pre-existing` or `unknown`. Unknown means the comparison did not establish provenance.
Use original IDs prefixed by the assignment ID. Coordinator-originated claims also get an intake assignment and saved notes. A reviewer
may preserve a submitted user concern as an independently investigated original. When no
reviewer owns that concern, the coordinator records it in intake. Intake counts as neither
an independent perspective nor a verifier. Original candidates are provisional claims, not
verdicts. For a concern contradicted by guards, preserve its stated trigger and impact, put
the contradiction in counterevidence, and leave the final disposition to the verifier.

Priorities: P0 is immediate severe failure supported by concrete evidence; P1 is serious
failure in a supported workflow; P2 is material impact under specific conditions; P3 is
limited but supported and actionable. Keep cosmetic preferences out.

The shared `findings.json` is an object with `candidates` and `duplicates` lists. A canonical
candidate retains the original candidate fields, but its `id` is `DR-NNN` and it adds:

```json
{
  "original_ids": ["frontend-a-001", "architecture-b-002"],
  "originators": ["first runtime identity", "second runtime identity"],
  "verdict": "unresolved", "verification": null, "stale": false
}
```

Every original appears in exactly one canonical record, including rejected concerns. For
merged candidates, `duplicates` has one `{ "original_id": "...", "canonical_id": "DR-001" }`
entry for every merged original, including the first. No mapping is needed for singletons.
Preserve differing evidence and counterevidence when merging. Null verification is allowed
only in a partial report with an unresolved verdict and a recorded limitation.

## Verification

Each `verification/<candidate>.json` contains:

```json
{
  "candidate_id": "DR-001", "agent_id": "independent verifier runtime identity",
  "verdict": "confirmed", "basis": "executed",
  "evidence": ["agents/verify-001/reproduction.txt"],
  "counterevidence": [], "limitations": [],
  "commands": [{"command": "python3 probe.py", "cwd": "named synthetic fixture",
                "exit_code": 0, "observation": "Specific claim established by the probe"}],
  "verified_hashes": {"src/example.ts": "actual SHA-256"}
}
```

Verifier identity must differ from every originator and belong to a completed verification
assignment. `basis` is `executed`, `code-and-contract`, `both` or `unavailable`. Decided
claims require available evidence. An unresolved verdict needs a limitation naming the
missing fact. `commands` is empty when no code ran, with the reason in notes. Include hashes
of every file needed for the verdict, including guards and callers. If source changed,
recheck against current hashes or retain old hashes and mark `stale: true`, `unresolved`.
The validator checks structured references and recorded identities; it cannot prove agent
honesty, semantic equivalence of duplicates, or completeness of an unrecorded dependency map.

## Report

`report.md` links the records and relevant evidence. Include status and scope, confirmed
counts by priority, rejected and unresolved counts, coverage by area, relevant documentation,
commands/results, limitations, source drift and suggested corrections. Prioritized findings
contain confirmed claims only. Separate unresolved concerns and preserve rejected evidence.
Distinguish introduced regressions from pre-existing issues. A report with unresolved claims
can be complete if independent investigation and all coverage/handoffs finished; explain
what remains unknown. Interrupted assignments or missing independent verification are partial.
