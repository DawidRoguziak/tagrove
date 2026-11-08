export const meta = {
  name: "audit-static-verify",
  description: "Tagrove /audit stage 3: exactly one refuter (review-verifier) and one judge (audit-judge) per static record",
  whenToUse: "Only from the /audit skill (static-verify stage) with args from `audit_state.py pending --ids static-verify`.",
  phases: [
    { title: "Refute", detail: "review-verifier, model by finder priority" },
    { title: "Judge", detail: "audit-judge, model by priority and dispute" }
  ]
};

// args: { stage, run, items: [{ id, state, needs: "refute"|"judge", role, priority }], models }
// A record already refuted skips the refuter, so it never gets a second one.
const input = args ?? {};
const run = input.run;
const items = Array.isArray(input.items) ? input.items : [];
const models = input.models ?? {};
const S = ".claude/skills/audit/scripts";
const JUDGE_ROLES = ["judge_high", "judge_low"];

const REFUTED = {
  type: "object",
  properties: {
    ok: { type: "boolean", description: "true only if put refutation printed ok" },
    verdict: { type: "string", enum: ["confirmed", "rejected", "uncertain"] },
    judge_role: { type: "string", description: "copy judge_role exactly from the put output" },
    error: { type: "string" }
  },
  required: ["ok"]
};
const JUDGED = {
  type: "object",
  properties: {
    ok: { type: "boolean", description: "true only if put judgment printed ok" },
    verdict: { type: "string" },
    error: { type: "string" }
  },
  required: ["ok"]
};

function opts(role, schema, extra) {
  const chosen = models[role];
  if (!chosen?.model) throw new Error(`args.models has no entry for role ${role}`);
  return { model: chosen.model, effort: chosen.effort, schema, ...extra };
}

function refutePrompt(item) {
  return [
    `Tagrove /audit static verification. Run directory: ${run}. Candidate record: ${run}/records/${item.id}.json.`,
    "Read the record's `finding` (the claim) and nothing else from docs/review/. Do not read other records,",
    "raw finder files or raports/. Apply your method to this one claim against the current tree.",
    "Audit mode: origin is pre-existing/recon/tool, so do not reject a finding for not being introduced by a change;",
    "see .claude/skills/audit/references/finding-format.md.",
    "",
    "Store your verdict with this single command; it is the only write you may make and an explicit exception",
    "to your read-only rules:",
    "```sh",
    `python3 ${S}/audit_state.py --run ${run} put refutation <<'EOF'`,
    `{"id": "${item.id}", "verdict": "confirmed|rejected|uncertain", "priority": "P0-P3", "reasoning": "...",`,
    ' "evidence": ["path:line ..."], "counterevidence": ["..."], "missing_fact": null, "new_issue": null}',
    "EOF",
    "```",
    "missing_fact is required when uncertain. Return ok, verdict and the judge_role printed by put."
  ].join("\n");
}

function judgePrompt(item) {
  return [
    `Judge record ${item.id}. Run directory: ${run}. Record: ${run}/records/${item.id}.json; duplicates index:`,
    `${run}/dup-index.json. Follow your agent instructions and store the judgment with`,
    `python3 ${S}/audit_state.py --run ${run} put judgment <<'EOF' ... EOF. Return ok and the verdict.`
  ].join("\n");
}

async function main() {
  if (!run || !items.length) {
    log("Nothing to do.");
    return { records: 0 };
  }
  const failed = [];
  const verdicts = {};
  await pipeline(
    items,
    async (item) => {
      if (item.needs === "judge") return { judge_role: item.role };
      const result = await agent(
        refutePrompt(item),
        opts(item.role, REFUTED, { agentType: "review-verifier", label: `refute ${item.id}`, phase: "Refute" })
      );
      if (!result?.ok) {
        failed.push(`${item.id}:refute`);
        throw new Error(`refuter failed for ${item.id}`);
      }
      return result;
    },
    async (refuted, item) => {
      // Fall back to the stronger judge when the refuter did not relay the role.
      const role = JUDGE_ROLES.includes(refuted.judge_role) ? refuted.judge_role : "judge_high";
      const result = await agent(
        judgePrompt(item),
        opts(role, JUDGED, { agentType: "audit-judge", label: `judge ${item.id}`, phase: "Judge" })
      );
      if (!result?.ok) {
        failed.push(`${item.id}:judge`);
        return null;
      }
      verdicts[result.verdict ?? "unknown"] = (verdicts[result.verdict ?? "unknown"] ?? 0) + 1;
      return result;
    }
  );
  const summary = { stage: "static-verify", records: items.length, verdicts, failed };
  log(`audit-static-verify summary: ${JSON.stringify(summary)}`);
  return summary;
}

await main();
