# Record contract (schema version 2)

The coordinator writes every record. Specialists are read-only, and their replies are saved
verbatim. JSON paths that refer to run artifacts are relative to the run directory and must
resolve inside it. Source paths and hash keys are repository-relative. Runs with schema
version 1 are historical and are not re-validated.

```
raports/<UTC>-<slug>/
  run.json  context.md  findings.json  report.md
  agents/<assignment>/{notes.md, findings.json[, collaboration.md, evidence...]}
  verification/<DR-NNN>.json [+ runtime logs]
```

## run.json

| Field | Content |
| --- | --- |
| `schema_version` | `2` |
| `status` | `partial` or `complete`. Partial requires non-empty `limitations` |
| `scope` | `request`, `mode` (`changes`/`paths`), `targets`, `included`, `supporting`, `exclusions`, `no_changes` |
| `baseline` | `head`, `comparison_base`, `committed`, `staged`, `unstaged`, `untracked`, `hashes` |
| `assignments` | see below |
| `coverage` | `[{id, paths, assignments}]`. Every included file appears in some area |
| `handoffs` | optional collaboration: `[{id, sender, recipient, question, evidence, response, status}]` |
| `runtime_owner` | the agent ID of the runtime assignment, or null |
| `source_changes` | files whose current hash differs from the baseline |
| `limitations` | gaps: missing tools, blocked agents, unverifiable facts |

Hashes are SHA-256 strings, `symlink:<hash>` for links, or null for deleted files. Add
supporting files with `snapshot(root, names)` from `init_review.py` before you assign them.

`no_changes: true` is valid only in `changes` mode, with empty `included`, `staged`,
`unstaged`, `untracked` and `committed` lists, no candidates, and recorded `comparison_base`
and `head` commits whose trees are equal. The validator checks the trees with git. If the
range can't be verified, set `no_changes: false` and record the limitation instead.

### Assignments

```json
{
  "id": "rust-a", "role": "review", "agent_type": "review-rust", "lens": "review-rust",
  "agent_id": "returned agent ID", "areas": ["backend"], "status": "complete",
  "model": "opus", "effort": "high",
  "notes": "agents/rust-a/notes.md", "findings": "agents/rust-a/findings.json",
  "limitations": []
}
```

- `role` is one of `review`, `verification`, `runtime`, `discovery`, `intake`. The
  coordinator's own recorded concerns use `intake`, with the coordinator's agent ID or
  `"coordinator"`.
- `status` is one of `pending`, `running`, `complete`, `blocked`, `not-applicable`. Any
  status other than complete needs a reason in `limitations`.
- `effort` is required for review and verification roles: `high`, `xhigh` or `max`.
- A complete assignment has its `notes` and `findings` files. `findings` may be `[]`, and
  verifier assignments may use `[]`.
- In a complete run, each coverage area has at least two complete `review` assignments with
  distinct `agent_id` **and** distinct `lens`.

## Original findings

Each `agents/<id>/findings.json` holds the agent's array, in the format defined in
[signal-bar.md](../../review-changes/references/signal-bar.md). Each entry has `id`, `lens`,
`path`, `line`, `title`, `priority`, `confidence`, `origin`, `trigger`, `impact`, `evidence`,
`counterevidence` and `fix`. `evidence` is a non-empty list of quoted code or rules. An
optional `evidence_files` list holds run-relative paths, which must exist. Original IDs are
unique across the run.

## findings.json

`{"candidates": [...], "duplicates": [...]}`. A canonical candidate has all the original
fields plus:

```json
{
  "id": "DR-001", "original_ids": ["rust-1", "conv-2"], "originators": ["agent A", "agent B"],
  "verdict": "confirmed", "verification": "verification/DR-001.json", "stale": false
}
```

- Every original ID belongs to exactly one candidate, rejected ones included. `originators`
  equals the set of agent IDs that produced those originals.
- `duplicates` has one `{original_id, canonical_id}` per original of every merged candidate
  with more than one original.
- `verdict` is one of `confirmed`, `rejected` or `uncertain`. A null `verification` is
  allowed only in a partial run with `uncertain`.

## verification/DR-NNN.json

```json
{
  "candidate_id": "DR-001", "agent_id": "verifier agent ID", "verdict": "confirmed",
  "basis": "code", "priority": "P1", "origin": "introduced",
  "evidence": ["path:line quoted code", "guards checked"], "evidence_files": [],
  "counterevidence": [], "missing_fact": null,
  "commands": [{"command": "...", "cwd": "...", "exit_code": 0, "observation": "..."}],
  "verified_hashes": {"src/example.ts": "sha256"}
}
```

- The verifier is a complete `verification` assignment and is not among the candidate's
  originators.
- `basis` is one of `code`, `executed`, `both` or `unavailable`. `executed` and `both`
  require `commands`. A decided verdict can't have `unavailable`.
- `uncertain` requires `missing_fact`.
- `verified_hashes` covers the finding's path and every guard or caller the verdict depends
  on, and every one of those must be in the baseline. If the hashes no longer match the
  current files, re-verify, or set `stale: true` and `uncertain`.

## report.md

The report covers the scope and status; confirmed findings by priority with their
corrections and origin; uncertain items with the missing fact; the rejected count; coverage
by area and lens; documentation used; commands and results; source drift; limitations; and
cleanup. Only confirmed claims appear as findings. The validator checks structure and
identities, but it can't prove that an agent was honest or that the dependency map is
complete.
