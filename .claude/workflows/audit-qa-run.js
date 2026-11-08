export const meta = {
  name: "audit-qa-run",
  description: "Tagrove /audit stage 5: QA blocks strictly in order; per session start, reflink copy, suites by qa-tester, guards, always stop; then log triage",
  whenToUse: "Only from the /audit skill (qa-run stage) with args from `audit_state.py pending --ids qa-run`.",
  phases: [
    { title: "Session", detail: "clerk: reap, prebuild, start, wait, copy, guard" },
    { title: "Suites", detail: "qa-tester per suite, guard after destructive suites" },
    { title: "Stop", detail: "clerk: stop and guard, always" },
    { title: "Triage", detail: "clerk assigns log errors to suites" }
  ]
};

// args: { stage, run, items: [{ id: "S1", title, tester_role, copy, edge, sessions: [{ perf, suites: [...] }],
//         triage_done }], models }
// Sessions never overlap: every await below is sequential on purpose.
const input = args ?? {};
const run = input.run;
const blocks = Array.isArray(input.items) ? input.items : [];
const models = input.models ?? {};
const S = ".claude/skills/audit/scripts";

const SESSION = {
  type: "object",
  properties: {
    ok: { type: "boolean", description: "true only if wait reported ready and copy and guard succeeded" },
    session: { type: "string", description: "the 32-hex session ID from qa_session.py wait" },
    media: { type: "string", description: "the media path from qa_session.py wait" },
    guard_clean: { type: "boolean" },
    error: { type: "string" }
  },
  required: ["ok"]
};
const GUARD = {
  type: "object",
  properties: { ok: { type: "boolean" }, clean: { type: "boolean" }, error: { type: "string" } },
  required: ["ok", "clean"]
};
const STOPPED = {
  type: "object",
  properties: { ok: { type: "boolean" }, guard_clean: { type: "boolean" }, error: { type: "string" } },
  required: ["ok"]
};
const SUITE = {
  type: "object",
  properties: {
    ok: { type: "boolean", description: "true only if put qa-suite printed ok" },
    status: { type: "string", enum: ["completed", "partial", "blocked", "aborted"] },
    bugs: { type: "integer" },
    session_usable: { type: "boolean", description: "false if doctor failed, the session is tainted or the app is gone" },
    error: { type: "string" }
  },
  required: ["ok"]
};
const ACK = {
  type: "object",
  properties: { ok: { type: "boolean" }, count: { type: "integer" }, error: { type: "string" } },
  required: ["ok"]
};

function opts(role, schema, extra) {
  const chosen = models[role];
  if (!chosen?.model) throw new Error(`args.models has no entry for role ${role}`);
  return { model: chosen.model, effort: chosen.effort, schema, ...extra };
}

const clerk = (prompt, schema, label, phase) =>
  agent(prompt, opts("clerk", schema, { agentType: "audit-clerk", label, phase }));

function startPrompt(block, perf) {
  const flag = perf ? " --perf" : "";
  return [
    `Start the QA session for block ${block.id} of the Tagrove /audit run ${run}. Stop at the first failure.`,
    `1. python3 ${S}/qa_session.py --run ${run} reap   (must print ok true)`,
    `2. python3 ${S}/qa_session.py --run ${run} prebuild${flag}   (long; must print ok true)`,
    `3. python3 ${S}/qa_session.py --run ${run} start --block ${block.id}${flag}`,
    `4. python3 ${S}/qa_session.py --run ${run} wait   (repeat while it prints "status": "starting"; at most 6 calls)`,
    `5. python3 ${S}/qa_session.py --run ${run} guard --label before-copy-${block.id}   (must print clean true)`,
    `6. python3 ${S}/qa_fs.py --run ${run} copy --block ${block.id}`,
    "Return ok, the session ID and media path from step 4, and guard_clean from step 5.",
    "If any step fails, return ok=false with the command, its exit code and the last lines of output."
  ].join("\n");
}

function suitePrompt(block, suite, session, fresh) {
  const lines = [
    `Run QA suite ${suite.id} "${suite.title}" of block ${block.id}. Run directory: ${run}.`,
    `Session ID: ${session.session}. Session media root (the only scan root): ${session.media}.`,
    `Catalog: ${suite.scenario_ref}. Destructive: ${suite.destructive ? "yes" : "no"}.`,
    `Mark boundaries: python3 ${S}/qa_session.py --run ${run} mark --block ${block.id} --suite ${suite.id} begin|end`
  ];
  if (suite.focus?.length) lines.push(`Extra focus: ${suite.focus.join("; ")}`);
  if (suite.targets?.length)
    lines.push(`Targets: reproduce ${suite.targets.map((id) => `${run}/records/${id}.json`).join(", ")} and report what you observe for each.`);
  if (fresh)
    lines.push(
      `The block's dataset folders were just copied into ${session.media}/dataset/ and are not indexed yet. ` +
        (suite.id === "QA-01"
          ? "Indexing them from the UI is this suite's first scenario."
          : "First open Settings, run Rescan all, wait until the scan finishes, and return to the gallery.")
    );
  lines.push(
    `Store the report with python3 ${S}/audit_state.py --run ${run} put qa-suite <<'EOF' ... EOF and return ok,`,
    "status, the number of bugs and whether the session is still usable."
  );
  return lines.join("\n");
}

async function stopSession(block, why) {
  const result = await clerk(
    [
      `Stop the QA session of the Tagrove /audit run ${run} (${why}).`,
      `Run python3 ${S}/qa_session.py --run ${run} stop once. Return ok and guard_clean exactly as printed.`,
      "If it fails, return ok=false with the output. Do not retry and do not try anything else."
    ].join("\n"),
    STOPPED,
    `stop ${block.id}`,
    "Stop"
  );
  return Boolean(result?.ok && result.guard_clean !== false);
}

async function runSession(block, plan, summary) {
  const session = await clerk(startPrompt(block, plan.perf), SESSION, `start ${block.id}${plan.perf ? " perf" : ""}`, "Session");
  let abort = false;
  try {
    if (!session?.ok || !session.session || session.guard_clean === false) {
      summary.failed.push(`${block.id}:session`);
      log(`${block.id}: session did not become ready: ${session?.error ?? "agent failed"}`);
      return true;
    }
    let fresh = true;
    for (const suite of plan.suites) {
      const result = await agent(
        suitePrompt(block, suite, session, fresh),
        opts(block.tester_role, SUITE, { agentType: "qa-tester", label: `${block.id} ${suite.id}`, phase: "Suites" })
      );
      fresh = false;
      if (result?.ok) summary.suites.push(suite.id);
      else summary.failed.push(suite.id);
      summary.bugs += result?.bugs ?? 0;
      if (suite.destructive) {
        const guard = await clerk(
          `Run python3 ${S}/qa_session.py --run ${run} guard --label after-${suite.id} and return ok and clean as printed.`,
          GUARD,
          `guard ${suite.id}`,
          "Suites"
        );
        if (!guard?.clean) {
          log(`${suite.id}: dataset guard is not clean; aborting the QA stage`);
          abort = true;
          break;
        }
      }
      if (result && result.session_usable === false) {
        log(`${suite.id}: session no longer usable; the remaining suites of this session wait for the next run`);
        break;
      }
    }
  } finally {
    // Always stop, even after a failed start: stop is a no-op without an active session.
    const stopped = await stopSession(block, abort ? "guard failure" : "end of session");
    if (!stopped) {
      log(`${block.id}: stop or post-stop guard failed; the run is halted`);
      abort = true;
    }
  }
  return abort;
}

async function main() {
  if (!run || !blocks.length) {
    log("Nothing to do.");
    return {};
  }
  const summary = { stage: "qa-run", suites: [], failed: [], bugs: 0, blocks: [], aborted: false };
  for (const block of blocks) {
    for (const plan of block.sessions) {
      if (await runSession(block, plan, summary)) {
        summary.aborted = true;
        break;
      }
    }
    if (summary.aborted) break;
    const triage = await clerk(
      [
        `Triage QA block ${block.id} of the Tagrove /audit run ${run}. Follow the 'Triage (one QA block)' section of your`,
        `instructions: python3 ${S}/qa_session.py --run ${run} triage --block ${block.id}, read the raw triage and the`,
        `block's suite reports in ${run}/qa/runs/${block.id}/, then put triage. If triage.json already exists, return ok.`,
        "Return ok and the number of new candidates."
      ].join("\n"),
      ACK,
      `triage ${block.id}`,
      "Triage"
    );
    if (!triage?.ok) summary.failed.push(`${block.id}:triage`);
    summary.blocks.push(block.id);
  }
  log(`audit-qa-run summary: ${JSON.stringify(summary)}`);
  return summary;
}

await main();
