# review-team-skill / architecture

Started: 2026-09-17T19:41:16Z

Review status: complete

Role status: complete. Assignment `AR-A1`, agent `/root/architecture`, fresh context. The coordinator `/root` independently read the same eight files and routing change. No candidate findings.

The reviewer inspected all eight skill files, the routing addition at [AGENTS.md:31](../../../AGENTS.md#L31), and the relevant test-level documentation. The tracked routing diff was compared with `32511af81e37b1f6ba9c099086050447303a2a44`; the new skill files were reviewed in their current untracked state. Previous and current review reports were excluded from investigation.

## Workflow coverage

| Case | Contract trace | Result |
| --- | --- | --- |
| Delegation unavailable | [SKILL.md:17](../../../.agents/skills/review-team/SKILL.md#L17), lines 44, 54, 74 | Record blocked capacity and a partial review; do not simulate agents. |
| Interrupted assignment or pending confirmation | [SKILL.md:74](../../../.agents/skills/review-team/SKILL.md#L74), [report-format.md:27](../../../.agents/skills/review-team/references/report-format.md#L27), lines 33, 44 | Remaining work stays visible and the invocation stays partial. |
| Duplicate candidates | [SKILL.md:58](../../../.agents/skills/review-team/SKILL.md#L58), [report-format.md:9](../../../.agents/skills/review-team/references/report-format.md#L9), lines 37, 49 | Merge only matching cause, trigger, and impact; preserve aliases and evidence. |
| Contradictory evidence | [SKILL.md:66](../../../.agents/skills/review-team/SKILL.md#L66), [confirmation.md:5](../../../.agents/skills/review-team/references/confirmation.md#L5), line 15 | Keep conflicting evidence and leave an unreconciled claim unresolved. |
| Unproven defect with an established design risk | [SKILL.md:60](../../../.agents/skills/review-team/SKILL.md#L60), [confirmation.md:7](../../../.agents/skills/review-team/references/confirmation.md#L7) | Keep separate claims and verdicts; a risk cannot substitute for an unproven runtime claim. |
| New candidate from a confirmer | [SKILL.md:66](../../../.agents/skills/review-team/SKILL.md#L66), [confirmation.md:19](../../../.agents/skills/review-team/references/confirmation.md#L19) | Send the new claim to another fresh confirmer; self-confirmation is prohibited. |
| Completed investigation with unresolved evidence | [SKILL.md:74](../../../.agents/skills/review-team/SKILL.md#L74) | Complete is allowed only after assignments and independent verdicts are accounted for; unresolved evidence remains visible. |
| Ordinary review routing and worker recursion | [AGENTS.md:31](../../../AGENTS.md#L31), [SKILL.md:12](../../../.agents/skills/review-team/SKILL.md#L12) | The boss delegates; assigned workers keep their roles and do not launch another team. |
| Generated files and report ownership | [init_review.py:27](../../../.agents/skills/review-team/scripts/init_review.py#L27), [report-format.md:3](../../../.agents/skills/review-team/references/report-format.md#L3) | Six partial skeletons match the required filenames. The boss later supplies report contents. |
| Metadata and role references | [openai.yaml:1](../../../.agents/skills/review-team/agents/openai.yaml#L1), [SKILL.md:48](../../../.agents/skills/review-team/SKILL.md#L48) | Display name, default prompt, role paths, and review routing agree. |

## Checks and limits

A read-only structural check passed for ten local Markdown links, the routing target, the six initializer filenames, and the referenced test-level anchor. `git diff --check 32511af81e37b1f6ba9c099086050447303a2a44 -- AGENTS.md .agents/skills/review-team` exited 0 from the repository root. That Git command covers the tracked routing diff, not untracked skill contents.

Evidence for workflow behavior is code-and-contract analysis. Actual delegation failures, interrupted runs, contradictory agent responses, and candidate confirmation were not injected. These are instructions for agents, not an executable state machine. No application tests ran for this scope. See [verification](verification.md) for executed initializer and integrity checks.
