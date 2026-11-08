export const meta = {
  name: "audit-qa-prep",
  description: "Tagrove /audit stage 4: serial desktop E2E sweep (clerk) in parallel with the QA planner (read-only)",
  whenToUse: "Only from the /audit skill (qa-prep stage) with args from `audit_state.py pending --ids qa-prep`.",
  phases: [
    { title: "Sweep", detail: "run_e2e_sweep.py, one spec at a time" },
    { title: "Plan", detail: "QA planner writes qa/plan.json" }
  ]
};

// args: { stage, run, items: [{ id: "sweep"|"plan", type, role, static_targets? }], models }
const input = args ?? {};
const run = input.run;
const items = Array.isArray(input.items) ? input.items : [];
const models = input.models ?? {};
const S = ".claude/skills/audit/scripts";

const ACK = {
  type: "object",
  properties: {
    ok: { type: "boolean" },
    count: { type: "integer", description: "items run or suites planned" },
    error: { type: "string" }
  },
  required: ["ok"]
};

function opts(role, extra) {
  const chosen = models[role];
  if (!chosen?.model) throw new Error(`args.models has no entry for role ${role}`);
  return { model: chosen.model, effort: chosen.effort, schema: ACK, ...extra };
}

function sweepPrompt() {
  return [
    `Tagrove /audit E2E sweep. Run directory: ${run}.`,
    `1. Check that artifacts/app-control/active.lock does not exist; if it does, return ok=false with that reason.`,
    `2. Run \`python3 ${S}/bg.py --run ${run} start sweep\`, then \`python3 ${S}/bg.py --run ${run} wait sweep\` again and`,
    "   again (each call returns within 9 minutes; the sweep runs every spec serially and can take hours) until it",
    "   reports status exited. Do not use sleep.",
    `3. Read ${run}/qa/sweep/summary.json and return ok=true with the number of items. If summary.json is missing,`,
    "   return ok=false with the last 30 lines of the command output."
  ].join("\n");
}

function planPrompt(item) {
  const targets = item.static_targets?.length ? item.static_targets.join(", ") : "none";
  return [
    `You plan the QA click-through of the Tagrove /audit run in ${run}. Work read-only except for one command at the end.`,
    "Read: docs/development/audit/qa-scenarios.md (the catalog, suites QA-01..QA-16),",
    ".claude/skills/audit/references/qa-blocks.json (blocks, destructive and perf_build suites),",
    ".claude/skills/audit/references/finding-format.md (plan format), .claude/skills/audit/references/qa-driving.md.",
    `Static records judged confirmed or unproven: ${targets}. Read each in ${run}/records/<id>.json.`,
    "",
    "1. Include every catalog suite QA-01..QA-16 in its block, with scenario_ref pointing to its catalog anchor.",
    "   Keep destructive and perf_build exactly as qa-blocks.json says. Add `focus` items where a static record",
    "   touches the suite's feature.",
    "2. Add targeted suites QA-T1, QA-T2, ... for records whose verdict is unproven (the missing_fact is a runtime",
    "   fact a tester can observe) and for confirmed findings visible in the UI. Put each in the block whose copy and",
    "   session fit it (S2 for anything that deletes, renames, imports or clears). One suite may target several records.",
    "   Skip records that cannot be observed through the UI, the database or the logs; list them in your reply.",
    "3. Store the plan with this single command:",
    `   python3 ${S}/audit_state.py --run ${run} put plan <<'EOF' ... EOF`,
    "Return ok and the number of suites."
  ].join("\n");
}

async function main() {
  if (!run || !items.length) {
    log("Nothing to do.");
    return {};
  }
  const sweep = items.find((item) => item.type === "sweep");
  const plan = items.find((item) => item.type === "plan");
  const [swept, planned] = await parallel([
    async () => (sweep ? agent(sweepPrompt(), opts(sweep.role, { agentType: "audit-clerk", label: "sweep", phase: "Sweep" })) : null),
    async () => (plan ? agent(planPrompt(plan), opts(plan.role, { label: "qa planner", phase: "Plan" })) : null)
  ]);
  const failed = [];
  if (sweep && !swept?.ok) failed.push("sweep");
  if (plan && !planned?.ok) failed.push("plan");
  const summary = { stage: "qa-prep", sweep: swept?.count ?? null, suites: planned?.count ?? null, failed };
  log(`audit-qa-prep summary: ${JSON.stringify(summary)}`);
  return summary;
}

await main();
