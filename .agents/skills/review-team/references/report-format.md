# Report format

The boss owns all writes under this invocation's directory. Keep reports concise and suitable for a public repository. All six files are required, even when a specialist is not applicable. Link reports using relative Markdown links. Show repository-relative `path:line` references and link the file with `../../../path` from the report folder. Lines refer to the recorded source baseline and may drift after edits.

## Candidate record

Every candidate includes:

- Stable original ID and assignment/agent identity; after merging, canonical `RT-NNN` ID plus every original alias and source reference.
- Title, priority, and classification `defect` or `design-risk`.
- Repository-relative file and line references, triggering conditions, impact, supporting evidence, and suggested correction.
- Known guards or counterevidence, assumptions, and missing facts. Link its canonical verdict in `verification.md#rt-nnn` from every specialist occurrence.

Use priorities consistently:

| Priority | Meaning |
| --- | --- |
| P0 critical | Immediate severe failure such as broadly reachable data loss or a critical security defect, supported by concrete evidence. |
| P1 high | Serious failure in a supported workflow requiring prompt correction. |
| P2 medium | Material defect or justified design risk under specific conditions. |
| P3 low | Limited impact, supported and actionable. Cosmetic preferences do not qualify. |

## README.md

Record scope title, UTC start, and status `partial` or `complete`. Link all five detailed reports. Summarize confirmed defects and confirmed design risks by priority, with canonical IDs linking to verification. Keep those classifications visible. List unresolved candidates separately, with links and the missing fact. State confirmed counts by priority and unresolved count. Mention rejected count with a link to verification.

Include coverage limitations, unavailable tests, interrupted or pending assignments, and remaining work. Do not describe an unreviewed area as passing. While work is active, say counts are provisional.

## scope.md

Record the sanitized user request, interpreted scope, UTC timestamps, starting commit or absence of Git metadata, comparison base if applicable, staged/unstaged/untracked changes, included and supporting paths, exclusions and reasons, documentation and contracts consulted, and source-preservation check.

Maintain an assignment table with assignment ID, role, agent identity, bounded subsystem/paths, status, actual inspected areas, checks performed, and skipped or blocked work. Use `pending`, `running`, `complete`, `blocked`, or `not applicable`. Explain not-applicable roles. A whole-project review needs an inventory mapped to assignments so an omitted subsystem stays visible. Record changes to scope or baseline and their effect on findings.

## frontend.md, backend.md, architecture.md

Record role status, assignments and agent identities, inspected paths/flows, tests and limitations, then all original candidate records. Preserve original references and aliases after merging; link each to its final canonical verification section. A rejected candidate remains here with a visible `rejected` status. A role with no candidates still reports coverage. An irrelevant role says `not applicable` and why.

## verification.md

Maintain one section per distinct candidate, headed `## RT-001` and so on for stable anchors. Include:

- Canonical candidate record, original aliases, originator identities, and links to specialist reports.
- Confirmation agent identity and verdict `confirmed`, `rejected`, or `unresolved`. Before confirmation completes, display `unresolved`, with confirmation `pending` or `blocked` as the reason.
- Supporting and contradicting evidence with repository-relative file/line references. Identify the evidence basis as `executed`, `code-and-contract`, or `both`.
- Exact test commands and working directory, exit result, concise sanitized observations, and what each test establishes. If execution was unavailable or no tests ran, say so and explain. Avoid copying private absolute paths or raw logs.
- Remaining uncertainty, missing verification, and the correction suggested by the evidence. Record any priority or classification change with its reason.

Deduplication never erases original IDs. A confirmer must differ from all originators. Unresolved candidates remain visible in README even if other candidates are confirmed. Rejected candidates never appear as confirmed summary findings.
