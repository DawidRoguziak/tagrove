export const meta = {
  name: "audit-qa-verify",
  description: "Tagrove /audit stage 6: QA bug dedup, then one qa-refuter per bug reproducing it in serialized verification sessions, then one audit-judge",
  whenToUse: "Only from the /audit skill (qa-verify stage) with args from `audit_state.py pending --ids qa-verify`.",
  phases: [
    { title: "Dedup", detail: "merge QA bugs that share a root cause" },
    { title: "Session", detail: "clerk starts verification sessions, one at a time" },
    { title: "Refute", detail: "qa-refuter, serialized through the session mutex" },
    { title: "Judge", detail: "audit-judge, concurrent" }
  ]
};

// args: { stage, run, items, models }. items is either [{ type: "qa-dedup", role, refs }] or
// records [{ id, state, needs: "refute"|"judge", role, priority, block, category }].
const input = args ?? {};
const run = input.run;
const items = Array.isArray(input.items) ? input.items : [];
const models = input.models ?? {};
const S = ".claude/skills/audit/scripts";
const JUDGE_ROLES = ["judge_high", "judge_low"];

const ACK = {
  type: "object",
  properties: { ok: { type: "boolean" }, count: { type: "integer" }, error: { type: "string" } },
  required: ["ok"]
};
const SESSION = {
  type: "object",
  properties: {
    ok: { type: "boolean" },
    session: { type: "string" },
    media: { type: "string" },
    guard_clean: { type: "boolean" },
    error: { type: "string" }
  },
  required: ["ok"]
};
const REFUTED = {
  type: "object",
  properties: {
    ok: { type: "boolean", description: "true only if put refutation printed ok" },
    verdict: { type: "string", enum: ["confirmed", "rejected", "uncertain"] },
    judge_role: { type: "string", description: "copy judge_role exactly from the put output" },
    session_usable: { type: "boolean" },
    error: { type: "string" }
  },
  required: ["ok"]
};
const JUDGED = {
  type: "object",
  properties: { ok: { type: "boolean" }, verdict: { type: "string" }, error: { type: "string" } },
  required: ["ok"]
};

function opts(role, schema, extra) {
  const chosen = models[role];
  if (!chosen?.model) throw new Error(`args.models has no entry for role ${role}`);
  return { model: chosen.model, effort: chosen.effort, schema, ...extra };
}
const clerk = (prompt, schema, label, phase) =>
  agent(prompt, opts("clerk", schema, { agentType: "audit-clerk", label, phase }));

// One verification session at a time: every refutation that needs the app runs inside this chain.
let chain = Promise.resolve();
function serial(task) {
  const next = chain.then(task);
  chain = next.catch(() => undefined);
  return next;
}
let current = null;
let aborted = false;

async function stopCurrent(why) {
  if (!current) return true;
  const stopped = await clerk(
    `Run python3 ${S}/qa_session.py --run ${run} stop once (${why}). Return ok and guard_clean as printed.`,
    { type: "object", properties: { ok: { type: "boolean" }, guard_clean: { type: "boolean" } }, required: ["ok"] },
    `stop ${current.block}`,
    "Session"
  );
  current = null;
  return Boolean(stopped?.ok && stopped.guard_clean !== false);
}

async function ensureSession(block, fresh) {
  if (current && current.block === block && !fresh && !current.dirty) return current;
  if (!(await stopCurrent("switching verification session"))) {
    aborted = true;
    return null;
  }
  const started = await clerk(
    [
      `Start a verification session for block ${block} of the Tagrove /audit run ${run}. Stop at the first failure.`,
      `1. python3 ${S}/qa_session.py --run ${run} reap`,
      `2. python3 ${S}/qa_session.py --run ${run} prebuild`,
      `3. python3 ${S}/qa_session.py --run ${run} start --block ${block}`,
      `4. python3 ${S}/qa_session.py --run ${run} wait   (repeat while "status": "starting"; at most 6 calls)`,
      `5. python3 ${S}/qa_session.py --run ${run} guard --label before-verify-${block}`,
      `6. python3 ${S}/qa_fs.py --run ${run} copy --block ${block}`,
      "Return ok, the session ID, the media path and guard_clean."
    ].join("\n"),
    SESSION,
    `session ${block}`,
    "Session"
  );
  if (!started?.ok || !started.session || started.guard_clean === false) {
    // Stop is a no-op without an active session, so always clean up a half-started one.
    current = { block };
    if (!(await stopCurrent("cleanup after a failed start"))) aborted = true;
    return null;
  }
  current = { block, session: started.session, media: started.media, dirty: fresh, indexed: false };
  return current;
}

function refutePrompt(item, session) {
  const lines = [
    `Try to defeat QA record ${item.id}. Run directory: ${run}. Record: ${run}/records/${item.id}.json.`
  ];
  if (session) {
    lines.push(`Verification session ID: ${session.session}. Media root (the only scan root): ${session.media}.`);
    if (!session.indexed)
      lines.push(
        `The block's folders were just copied into ${session.media}/dataset/ and are not indexed: open Settings,` +
          " run Rescan all and wait for it to finish before reproducing."
      );
  } else {
    lines.push("No session: this record comes from the E2E sweep. Decide from its log and the spec source only.");
  }
  lines.push(
    `Store with python3 ${S}/audit_state.py --run ${run} put refutation <<'EOF' ... EOF.`,
    "Return ok, the verdict, the judge_role printed by put, and whether the session is still usable."
  );
  return lines.join("\n");
}

async function refute(item) {
  if (aborted) throw new Error("aborted");
  const needsSession = item.block !== "sweep";
  const fresh = item.block === "S2" || item.category === "data-safety";
  let session = null;
  if (needsSession) {
    session = await ensureSession(item.block, fresh);
    if (!session) throw new Error(`no session for ${item.id}`);
  }
  const result = await agent(
    refutePrompt(item, session),
    opts(item.role, REFUTED, { agentType: "qa-refuter", label: `refute ${item.id}`, phase: "Refute" })
  );
  if (session) {
    session.indexed = true;
    if (result?.session_usable === false) session.dirty = true;
    if (fresh) {
      const guard = await clerk(
        `Run python3 ${S}/qa_session.py --run ${run} guard --label after-verify-${item.id} and return ok and clean.`,
        { type: "object", properties: { ok: { type: "boolean" }, clean: { type: "boolean" } }, required: ["ok", "clean"] },
        `guard ${item.id}`,
        "Session"
      );
      if (!guard?.clean) aborted = true;
    }
  }
  if (!result?.ok) throw new Error(`refuter failed for ${item.id}`);
  return result;
}

async function main() {
  if (!run || !items.length) {
    log("Nothing to do.");
    return {};
  }
  const dedup = items.find((item) => item.type === "qa-dedup");
  if (dedup) {
    const result = await agent(
      [
        `Deduplicate the QA bugs of the Tagrove /audit run ${run}. Work read-only except for one command.`,
        `Bug references: ${dedup.refs.join(", ")}. "S1/QA-05#2" is bugs[2] of ${run}/qa/runs/S1/QA-05.json;`,
        `"S1/QA-05#t0" is new_candidates[0] of ${run}/qa/runs/S1/triage.json (the one whose suite is QA-05).`,
        "Two bugs are duplicates only when they share the root cause (same broken behavior, possibly seen in",
        "different suites); similar symptoms with different causes stay separate. Keep the best-evidenced one.",
        `Store: python3 ${S}/audit_state.py --run ${run} put qa-dedup <<'EOF'`,
        '{"groups": [{"keep": "S1/QA-05#2", "merged": ["S2/QA-10#0"], "reason": "..."}]}',
        "EOF  (an empty groups list is valid). Return ok and the number of groups."
      ].join("\n"),
      opts(dedup.role, ACK, { label: "qa dedup", phase: "Dedup" })
    );
    log(`audit-qa-verify dedup: ${JSON.stringify(result)}. Next (coordinator): assign-ids --kind qa, then run qa-verify again.`);
    return { stage: "qa-verify", dedup: Boolean(result?.ok) };
  }
  const failed = [];
  const verdicts = {};
  // Records needing a session are grouped by block so one session serves many refuters.
  const ordered = [...items].sort((a, b) => String(a.block).localeCompare(String(b.block)));
  try {
    await pipeline(
      ordered,
      async (item) => {
        if (item.needs === "judge") return { judge_role: item.role };
        try {
          return await serial(() => refute(item));
        } catch (error) {
          failed.push(`${item.id}:refute`);
          throw error;
        }
      },
      async (refuted, item) => {
        const role = JUDGE_ROLES.includes(refuted.judge_role) ? refuted.judge_role : "judge_high";
        const result = await agent(
          `Judge record ${item.id}. Run directory: ${run}. Record: ${run}/records/${item.id}.json. Follow your agent ` +
            `instructions and store with python3 ${S}/audit_state.py --run ${run} put judgment <<'EOF' ... EOF. Return ok and the verdict.`,
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
  } finally {
    await serial(() => stopCurrent("end of verification"));
  }
  const summary = { stage: "qa-verify", records: items.length, verdicts, failed, aborted };
  log(`audit-qa-verify summary: ${JSON.stringify(summary)}`);
  return summary;
}

await main();
