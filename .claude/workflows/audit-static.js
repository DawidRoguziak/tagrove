export const meta = {
  name: "audit-static",
  description: "Tagrove /audit stage 2: tool runner, static finders per batch and lens, per-batch dedup; results stored with audit_state.py put",
  whenToUse: "Only from the /audit skill (static stage) with args from `audit_state.py pending --ids static`.",
  phases: [
    { title: "Tools", detail: "clerk runs run_tools.sh and stores tools/summary.json" },
    { title: "Find", detail: "one finder per batch and lens (models from args.models)" },
    { title: "Dedup", detail: "clerk merges duplicates inside each batch" }
  ]
};

// args: { stage: "static", run: "docs/review/<run>", items: [...], models: { role: { model, effort } } }
// Model and effort always come from args.models; this script never names a model.
const input = args ?? {};
const run = input.run;
const items = Array.isArray(input.items) ? input.items : [];
const models = input.models ?? {};
const S = ".claude/skills/audit/scripts";

const ACK = {
  type: "object",
  properties: {
    ok: { type: "boolean", description: "true only if audit_state.py put printed ok" },
    stored: { type: "string", description: "the path or ID put reported" },
    count: { type: "integer", description: "findings, groups or results stored" },
    error: { type: "string" }
  },
  required: ["ok"]
};

function opts(role, extra) {
  const chosen = models[role];
  if (!chosen?.model) throw new Error(`args.models has no entry for role ${role}`);
  return { model: chosen.model, effort: chosen.effort, schema: ACK, ...extra };
}

function finderPrompt(item) {
  return [
    `You are the "${item.lens}" lens of the Tagrove /audit static stage, batch ${item.batch} (${item.title}).`,
    `Run directory: ${run}. This is a whole-project audit, not a change review.`,
    "",
    "Read .claude/skills/audit/references/finding-format.md first (audit mode overrides the signal bar's",
    "change-only limits) and .claude/skills/review-changes/references/signal-bar.md.",
    "Route through the CLAUDE.md documentation index to the owning docs and .claude/rules files for these files.",
    "Do not read docs/review/, raports/ or any other agent's output.",
    "",
    `Assigned files (${item.files.length}); open each in full and follow calls out of the batch when needed:`,
    ...item.files.map((file) => `- ${file}`),
    "",
    "When done, store your result exactly once (also when there are no findings) with a quoted heredoc.",
    "This single command is the only write you may make; it is an explicit exception to your read-only rules:",
    "```sh",
    `python3 ${S}/audit_state.py --run ${run} put raw <<'EOF'`,
    `{"batch": "${item.batch}", "lens": "${item.lens}", "findings": [...], "leads": [...], "files_reviewed": [...]}`,
    "EOF",
    "```",
    "If put refuses, fix the JSON and retry once. Then return ok, the stored path and the finding count."
  ].join("\n");
}

function toolsPrompt() {
  return [
    `Tagrove /audit tool runner. Run directory: ${run}.`,
    `1. Run \`python3 ${S}/bg.py --run ${run} start tools\`, then \`python3 ${S}/bg.py --run ${run} wait tools\` again and`,
    "   again (each call returns within 9 minutes) until it reports status exited. Do not use sleep.",
    `2. Read ${run}/tools/raw-results.json and, for every result with status fail, the matching ${run}/tools/<name>.log.`,
    "3. Build the summary JSON: {\"results\": [ {name, status, exit_code, seconds, failures: [...]} ]}. Each failure is a",
    "   static finding (see .claude/skills/audit/references/finding-format.md) with file and line taken from the log:",
    "   failing tests and type errors priority P2 category bug; lint, format, clippy warnings priority P3 category convention;",
    "   origin \"tool\". At most 40 failures per tool; group identical messages. A tool with status unavailable has no failures.",
    `4. Store it: python3 ${S}/audit_state.py --run ${run} put tools <<'EOF' ... EOF`,
    "Return ok and the number of failures stored."
  ].join("\n");
}

function dedupPrompt(item) {
  return [
    `Tagrove /audit static dedup for batch ${item.batch}. Run directory: ${run}.`,
    `Read these raw files: ${item.raw.map((file) => `${run}/${file}`).join(", ")}.`,
    "Follow the 'Static dedup (one batch)' section of your agent instructions. Store with",
    `python3 ${S}/audit_state.py --run ${run} put dedup <<'EOF' ... EOF  (an empty groups list is valid).`,
    "Return ok and the number of groups."
  ].join("\n");
}

async function main() {
  if (!run || !items.length) {
    log("Nothing to do: args.run or args.items is empty.");
    return { finders: 0 };
  }
  const tools = items.find((item) => item.type === "tools");
  const finders = items.filter((item) => item.type === "finder");
  const dedups = items.filter((item) => item.type === "dedup");
  const batches = [...new Set([...finders.map((i) => i.batch), ...dedups.map((i) => i.batch)])];
  const failed = [];

  const toolsTask = async () => {
    if (!tools) return null;
    const result = await agent(toolsPrompt(), opts(tools.role, { agentType: "audit-clerk", label: "tools", phase: "Tools" }));
    if (!result?.ok) failed.push("tools");
    return result;
  };

  const batchTask = (batch) => async () => {
    const lenses = finders.filter((item) => item.batch === batch);
    const results = await parallel(
      lenses.map((item) => () =>
        agent(finderPrompt(item), opts(item.role, { agentType: item.agent, label: item.id, phase: "Find" }))
      )
    );
    const okIds = lenses.filter((_, index) => results[index]?.ok).map((item) => item.id);
    for (const item of lenses) if (!okIds.includes(item.id)) failed.push(item.id);
    const dedup = dedups.find((item) => item.batch === batch);
    if (!dedup) return;
    if (!dedup.after.every((id) => okIds.includes(id))) {
      log(`${batch}: dedup postponed until every lens of the batch is stored`);
      failed.push(dedup.id);
      return;
    }
    const merged = await agent(dedupPrompt(dedup), opts(dedup.role, { agentType: "audit-clerk", label: dedup.id, phase: "Dedup" }));
    if (!merged?.ok) failed.push(dedup.id);
  };

  await parallel([toolsTask, ...batches.map(batchTask)]);
  const summary = {
    stage: "static",
    finders: finders.length,
    dedups: dedups.length,
    tools: Boolean(tools),
    failed
  };
  log(`audit-static summary: ${JSON.stringify(summary)}`);
  log("Next (coordinator): audit_state.py assign-ids --kind static, dup-index, status.");
  return summary;
}

await main();
