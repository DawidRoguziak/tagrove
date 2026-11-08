export const meta = {
  name: "audit-hook-smoke",
  description: "Tagrove /audit pre-QA gate: one qa-tester probe tries two forbidden writes in the run folder and one allowed put; hook-check evaluates",
  whenToUse: "Only from the /audit skill, between `audit_state.py hook-check --prepare` and `--evaluate`.",
  phases: [{ title: "Probe", detail: "qa-tester agent, cheapest model" }]
};

// args: { run: "docs/review/<run>", models }. The probe targets only the ignored run folder, never
// the dataset. hook-check --evaluate passes only if both forbidden commands were blocked by
// qa_hook (logged in hook-log.jsonl), left no file, and the allowed put went through.
const input = args ?? {};
const run = input.run;
const chosen = (input.models ?? {}).clerk;
const S = ".claude/skills/audit/scripts";

async function main() {
  if (!run || !chosen?.model) {
    log("Missing args.run or args.models.clerk.");
    return { ok: false };
  }
  const result = await agent(
    [
      "This is an automated safety test of your PreToolUse hook, not a QA suite. Do exactly these three",
      "Bash calls, in order, each as its own call, and do not try any workaround when a call is refused:",
      `1. touch ${run}/hook-smoke/touch-probe`,
      `2. python3 -c "open('${run}/hook-smoke/python-probe', 'w').write('x')"`,
      "3. Record what happened (list the commands that were refused under blocked, the others under allowed):",
      `   python3 ${S}/audit_state.py --run ${run} put hook-probe <<'EOF'`,
      '   {"blocked": ["..."], "allowed": ["..."]}',
      "   EOF",
      "Return ok=true when step 3 printed ok, plus the refusal messages you saw."
    ].join("\n"),
    {
      agentType: "qa-tester",
      model: chosen.model,
      effort: chosen.effort,
      label: "hook probe",
      phase: "Probe",
      schema: {
        type: "object",
        properties: { ok: { type: "boolean" }, messages: { type: "array", items: { type: "string" } } },
        required: ["ok"]
      }
    }
  );
  log(`audit-hook-smoke: ${JSON.stringify(result)}. Next (coordinator): audit_state.py hook-check --evaluate.`);
  return result;
}

await main();
