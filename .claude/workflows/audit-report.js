export const meta = {
  name: "audit-report",
  description: "Tagrove /audit stage 7: report editor writes report.md from the judged records, status, coverage and leads",
  whenToUse: "Only from the /audit skill (report stage) with args from `audit_state.py pending --ids report`, after coverage.py and dup-index.",
  phases: [{ title: "Report", detail: "one editor agent" }]
};

// args: { stage, run, items: [{ id: "report", role }], models }
const input = args ?? {};
const run = input.run;
const items = Array.isArray(input.items) ? input.items : [];
const models = input.models ?? {};
const S = ".claude/skills/audit/scripts";

async function main() {
  const item = items.find((entry) => entry.type === "report");
  if (!run || !item) {
    log("Nothing to do.");
    return {};
  }
  const chosen = models[item.role];
  if (!chosen?.model) {
    log(`args.models has no entry for role ${item.role}`);
    return { failed: ["report"] };
  }
  const result = await agent(
    [
      `Write the final report of the Tagrove /audit run in ${run}. Work read-only except for one command.`,
      `Read: ${run}/STATUS.md, ${run}/static-findings.md, ${run}/qa-findings.md, ${run}/coverage.md,`,
      `${run}/dup-index.json, the judged records in ${run}/records/ (open the confirmed and unproven ones),`,
      `${run}/qa/plan.json, ${run}/qa/sweep/summary.json, ${run}/tools/summary.json, the raw finder leads`,
      `(${run}/raw/*.json "leads"), the refuters' new_issue fields, ${run}/guard/*.json and the smoke results in run.json.`,
      "Never copy the dataset path, home paths or private media names into the report; write $AUDIT_DATASET.",
      "",
      "Sections (English, concise, every claim linked to a record ID or file):",
      "1. Summary: scope, baseline commit, counts per stage and verdict, the most important confirmed issues.",
      "2. Confirmed findings by priority (P0 first), then category; one entry per record with title, location,",
      "   impact, the judge's reasoning in one or two sentences, and the suggested fix when given.",
      "3. Static ↔ QA links: records whose QA-T suite reproduced (or failed to reproduce) a static finding.",
      "4. Known issues (verdict known) with their sources.",
      "5. Unproven static findings and not-reproducible QA bugs, with the missing fact.",
      "6. Coverage: untouched controls (scale-only ones separately), skipped scenarios, sweep failures.",
      "7. Gaps: what this run could not prove (Xvfb limits, windowed mode at 30k, alert smoke, anything halted).",
      "8. Unverified leads.",
      `Store it with: python3 ${S}/audit_state.py --run ${run} put report <<'EOF' ... EOF`,
      "Return ok when put printed ok."
    ].join("\n"),
    {
      model: chosen.model,
      effort: chosen.effort,
      label: "report editor",
      phase: "Report",
      schema: { type: "object", properties: { ok: { type: "boolean" }, error: { type: "string" } }, required: ["ok"] }
    }
  );
  const summary = { stage: "report", ok: Boolean(result?.ok) };
  log(`audit-report summary: ${JSON.stringify(summary)}`);
  return summary;
}

await main();
