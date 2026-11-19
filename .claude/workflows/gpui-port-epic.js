export const meta = {
  name: "gpui-port-epic",
  description: "Tagrove GPUI port: one epic end to end - implement, port tests, check/fix loop, review loop (up to 6 rounds), test:all gate, record",
  whenToUse: "Only from the /gpui-port skill with args from `port_state.py args <EPIC>`. One epic per run, never two runs at once.",
  phases: [
    { title: "Implement", detail: "one implementer builds the epic scope" },
    { title: "Tests", detail: "a second agent ports the behaviour tests" },
    { title: "Stabilize", detail: "checks -> fixes until green; screenshot parity when the epic has scenarios" },
    { title: "Review", detail: "lenses, one verifier per finding, fixes; repeats until a clean round or the round limit" },
    { title: "Gate", detail: "bun run test:all on a private display -> fixes" },
    { title: "Record", detail: "epic notes, open-findings report, status" }
  ]
};

// args (from port_state.py args): { epic: { id, slug, title, file, size, tier, deps, sources, tests_sources, docs,
//   allowed_deps, parity, checks }, plan, run, base, date, from, maxFixRounds, maxReviewRounds, maxGateRounds,
//   maxParityRounds, parityAvailable, models: { role: { model, effort } }, dryRun }
// Agents that write run strictly one after another (no worktrees, shared target dirs and desktop locks);
// only the read-only review lenses and verifiers run in parallel.
const input = args ?? {};
const epic = input.epic ?? {};
const models = input.models ?? {};
const plan = input.plan ?? "docs/plans/gpui-port";
const base = input.base;
const date = input.date;
const DRY = input.dryRun === true;
const STEPS = ["implement", "tests", "stabilize", "review", "gate"];
const startAt = Math.max(0, STEPS.indexOf(input.from ?? "implement"));
const MAX_FIX = input.maxFixRounds ?? 5;
const MAX_REVIEW = input.maxReviewRounds ?? 6;
const MAX_GATE = input.maxGateRounds ?? 3;
const MAX_PARITY = input.maxParityRounds ?? 2;
const S = ".claude/skills/gpui-port/scripts";
const hard = epic.tier === "hard";
const fixerRole = hard ? "fixer_hard" : "fixer";
const epicFile = `${plan}/${epic.file}`;
const CORE_LENSES = ["review-conventions", "review-rust", "review-gpui"];
const REPORTED = ["P0", "P1", "P2"];

const str = { type: "string" };
const strs = { type: "array", items: str };
const bool = { type: "boolean" };
const int = { type: "integer" };
const PRIORITY = { type: "string", enum: ["P0", "P1", "P2", "P3"] };
const ORIGIN = { type: "string", enum: ["introduced", "pre-existing"] };
const obj = (properties, required) => ({ type: "object", properties, required });
const list = (properties, required) => ({ type: "array", items: obj(properties, required) });

const IMPLEMENTED = obj({
  ok: { type: "boolean", description: "true when the epic scope is built and the touched crates compile" },
  summary: str, files: strs, decisions: strs, deferred: strs,
  blocked: { type: "boolean", description: "true when you cannot proceed without the user" }, blocked_reason: str
}, ["ok", "summary"]);
const TESTED = obj({
  ok: bool, summary: str, tests_added: int, ported: int, files: strs,
  skipped: list({ case: str, reason: str }, ["case", "reason"]),
  failing: list({ test: str, why: str }, ["test", "why"])
}, ["ok", "summary"]);
const CHECKED = obj({
  ok: { type: "boolean", description: "true only when the job exited with code 0" },
  status: { type: "string", enum: ["passed", "failed", "busy", "error"] },
  failures: list({ command: str, summary: str }, ["command", "summary"]),
  log: str, error: str
}, ["ok", "status"]);
const FIXED = obj({
  ok: bool, summary: str, files: strs, unresolved: list({ item: str, reason: str }, ["item", "reason"])
}, ["ok", "summary"]);
const COMPARED = obj({
  ok: bool, available: { type: "boolean", description: "false when the comparison harness does not exist yet" },
  pairs: list({ scenario: str, tauri: str, gpui: str }, ["scenario", "tauri", "gpui"]), error: str
}, ["ok", "available"]);
const PARITY = obj({
  diffs: list({
    scenario: str, severity: { type: "string", enum: ["blocking", "notable", "minor"] }, where: str, expected: str, actual: str
  }, ["scenario", "severity", "where", "expected", "actual"])
}, ["diffs"]);
const SCOPE = obj({
  ok: bool, empty: bool, base: str, target: str, diff_command: str, untracked: strs,
  files: list({ path: str, category: str, hot_path: bool }, ["path"]),
  changed_lines: int, app_source_lines: int, suggested_lenses: strs, rules: strs, docs: strs, error: str
}, ["ok", "empty"]);
const FINDING = {
  id: str, lens: str, path: str, line: int, title: str, priority: PRIORITY,
  confidence: { type: "integer", description: "0-100, after checking guards" }, origin: ORIGIN,
  trigger: str, impact: str, evidence: strs, counterevidence: str, fix: str
};
const FINDINGS = obj({
  findings: list(FINDING, ["id", "path", "title", "priority", "confidence", "origin", "trigger", "impact", "fix"]),
  notes: { type: "string", description: "what you inspected beyond the diff, what you could not check, cross-lens questions" }
}, ["findings"]);
const VERDICT = obj({
  candidate: str, verdict: { type: "string", enum: ["confirmed", "rejected", "uncertain"] }, priority: PRIORITY, origin: ORIGIN,
  reasoning: str, evidence: strs, counterevidence: strs,
  missing_fact: { type: "string", description: "required when uncertain; empty otherwise" },
  new_issue: { type: "string", description: "a different issue you noticed, with path:line, trigger and impact; empty when none" }
}, ["verdict", "reasoning"]);
const SIDE_VERDICT = obj({
  verdict: { type: "string", enum: ["confirmed", "rejected", "uncertain"] },
  path: str, line: int, title: str, priority: PRIORITY, origin: ORIGIN, trigger: str, impact: str, fix: str, reasoning: str
}, ["verdict", "reasoning"]);
const REVIEW_FIXED = obj({
  ok: bool, summary: str, files: strs,
  fixed: list({ id: str, summary: str }, ["id", "summary"]),
  disputed: list({ id: str, reason: str }, ["id", "reason"])
}, ["ok"]);
const GATE_FIXED = obj({
  ok: bool, classification: { type: "string", enum: ["caused", "unrelated", "unclear"] }, summary: str, files: strs
}, ["ok", "classification", "summary"]);
const RECORDED = obj({ ok: bool, report: str, error: str }, ["ok"]);

const DRY_PROMPT = [
  "Dry run of the gpui-port-epic workflow: there is no real work.",
  "Do not use any tool other than the structured output, and do not read or change any file.",
  "Return the smallest valid object: every boolean true except `empty`, `blocked` and `available`, which are false;",
  "arrays empty; integers 0; strings \"dry run\"; for a field with an enum pick its first allowed value."
].join("\n");

const RULES = [
  "Standing rules of this port (they override convenience):",
  "- Never create a git worktree. Never commit, stage, push, stash, reset or check out anything; leave your changes in the working tree.",
  "- The Tauri app stays as it is: no behaviour, wire-contract or test changes under src/, e2e/ or src-tauri/src/ unless the epic file names the file. Changes to media_tagger_core are additive only.",
  "- Two workspaces: the port lives in src-gpui/ (cargo commands take --manifest-path src-gpui/Cargo.toml) with its own Cargo.lock; GPUI comes only from the zed-industries/zed repository at the pinned rev. src-tauri/Cargo.toml and src-tauri/Cargo.lock change only for an additive core change the epic file names, and never gain a GPUI or git dependency.",
  "- Layers: media_tagger_client has no GUI dependency and reaches the core only through its Backend port (backend/core_backend.rs is the one adapter file). media_tagger_gpui holds views and executor glue; it never touches media_tagger_core::db, lock helpers or use cases outside startup composition. Never block the UI thread on a backend call.",
  "- Models publish state only after the backend succeeded, from the backend's canonical result; a late reply is dropped by its request or generation token.",
  "- GPUI APIs move fast and your memory of them may be wrong: before using an API, read the source and examples of the pinned Zed commit in the cargo git checkout (~/.cargo/git/checkouts/zed-*/<rev>/crates/gpui/, rev from src-gpui/Cargo.toml).",
  "- cargo always with --locked. A new dependency is allowed only if the epic file lists it under 'Dozwolone nowe zależności' (then one cargo run without --locked may update Cargo.lock for exactly that dependency); otherwise stop and return blocked with the dependency you need.",
  "- User-visible text comes from src/i18n/locales (all 11 files, same keys and placeholders). No hardcoded strings.",
  "- Desktop runs are serial and private: a private Xvfb display with temporary XDG directories, never the user's real display or real app data; never two display-bound commands at once; never kill processes by name; never delete a lock you did not create; always stop what you started. `bun run test:all` and the full `bun run test:e2e:tauri` are run only by this workflow's gate.",
  "- Destructive behaviour (delete, clear, restore, duplicate resolution) is tested only on fresh temporary roots; never personal folders, the repository root or real app data.",
  "- The repository is public: no private paths, user names or real media names in code, docs, notes or fixtures.",
  "- A single command of yours stops after 10 minutes; keep your own runs targeted (one crate, one test filter)."
].join("\n");

function pick(role) {
  const chosen = models[role];
  if (!chosen?.model) throw new Error(`args.models has no entry for role ${role}`);
  return chosen;
}

async function ask(role, prompt, schema, extra) {
  const chosen = pick(role);
  if (DRY) {
    return agent(DRY_PROMPT, { model: pick("clerk").model, effort: "low", schema, label: `dry: ${extra.label}`, phase: extra.phase });
  }
  return agent(prompt, { model: chosen.model, effort: chosen.effort, schema, ...extra });
}

const names = (items) => (items?.length ? items.join(", ") : "none");
const json = (value) => JSON.stringify(value, null, 1);

function header() {
  return [
    `Tagrove GPUI port, epic ${epic.id} "${epic.title}".`,
    `Plan: ${plan}/README.md (sections "Stan docelowy", "Decyzje", "Zasady dla agentów" are binding). Epic file: ${epicFile}. Handover log: ${plan}/NOTES.md.`,
    `Everything this epic changed is \`git diff ${base}\` plus untracked files (\`git status --short\`).`
  ].join("\n");
}

function implementPrompt() {
  return [
    header(),
    "",
    "You implement this epic.",
    `1. Read the plan README sections named above, the epic file in full, and the 'Notatki agentów' of the epics it depends on (${names(epic.deps)}).`,
    `2. Read the behaviour sources: ${names(epic.sources)}. The Solid/Tauri implementation and its tests are the behaviour specification. Contracts: ${names(epic.docs)}.`,
    "3. Build everything under 'Zakres' so that every item under 'Kryteria akceptacji' holds. Stay inside the epic: write down what belongs to another epic instead of building it.",
    "4. The working tree may already hold partial work of this epic from an interrupted run; continue from it instead of starting over.",
    "5. Keep the code testable (pure functions and state machines in the client crate, thin views). You may write the tests you need while developing; a separate agent ports the behaviour test suite next.",
    "6. Before you finish: the crates you touched compile (cargo check --locked), `cargo fmt --manifest-path src-gpui/Cargo.toml --all` (and the src-tauri one if you touched the core) was run, and the owning docs the epic names are updated. Do not edit the epic's Status section, the README table or NOTES.md.",
    "",
    RULES,
    "",
    "Return ok (scope built and compiling), summary, files (created or changed), decisions (choices a later epic must know), deferred (scope items not finished, with reasons), and blocked + blocked_reason when you cannot proceed without the user: a missing system package, a dependency the epic does not allow, or a decision the plan does not cover."
  ].join("\n");
}

function testsPrompt(implemented) {
  return [
    header(),
    "",
    "You write the behaviour tests of this epic. You did not implement it: judge it by the specification, not by its code.",
    `1. Read the epic file ('Kryteria akceptacji', 'Testy i sprawdzenia'), then the TypeScript tests that specify the behaviour: ${names(epic.tests_sources)}, next to their sources: ${names(epic.sources)}.`,
    "2. Port every TS case that describes product behaviour to a Rust test at the lowest layer that proves it: pure logic and models -> unit tests in media_tagger_client on the deterministic executor and FakeBackend; workflows that need real SQLite -> client tests on a tempfile core; view behaviour (focus, keys, layer order, visible range) -> #[gpui::test] in media_tagger_gpui. Skip cases about Solid, DOM or webview mechanics and list each with its reason.",
    "3. Add a test for every acceptance criterion that no ported case covers and that a test can observe.",
    "4. Do not weaken or delete existing tests. Change production code only to make it testable (visibility, seams). When a test exposes a real defect, keep the failing test and report it under failing; the next step fixes it.",
    "5. Run what you wrote with targeted cargo test --locked: it must compile, and may fail only for real defects.",
    `Implementer's summary: ${implemented?.summary ?? "not available (this run started after the implement step)"}`,
    "",
    RULES,
    "",
    "Return ok, summary, tests_added, ported (TS cases ported), files, skipped [{case, reason}], failing [{test, why}]."
  ].join("\n");
}

function jobPrompt(job, extraArgs, what) {
  return [
    `Run one job for the Tagrove GPUI port epic ${epic.id} and report. You never edit files and never try to fix anything.`,
    `1. \`python3 ${S}/job.py start ${epic.id} ${job}${extraArgs}\`. If it prints status "busy" or an error, return ok=false with status busy or error and that text.`,
    `2. \`python3 ${S}/job.py wait ${epic.id} ${job}\` again and again until status is "exited". Each call returns within 9 minutes; ${what} Do not use sleep and do not run the commands yourself.`,
    "3. When it exited with code 0 return ok=true, status passed.",
    "4. Otherwise read the log (path in the wait output) around each failed step and return ok=false, status failed, and failures: one entry per failed command with a summary of at most 20 lines that keeps the decisive text verbatim (failing test names, assertion messages, compiler or clippy errors with file:line). Also return the log path."
  ].join("\n");
}

function fixPrompt(failures, log, round, context) {
  return [
    header(),
    "",
    `${context} (fix round ${round}). Failures:`,
    json(failures ?? []),
    `Full log: ${log ?? "see docs/review/gpui-port/" + epic.id + "/jobs/"}`,
    "",
    "Find the root cause of each failure and fix it where it is wrong: production code or the test, judged by the epic file and the Solid behaviour. Never make a check pass by weakening an assertion, by #[ignore], by deleting a test, or by #[allow(...)] without a comment that states the trade-off. Re-run the failing commands, narrowed to the failing tests, until they pass.",
    "",
    RULES,
    "",
    "Return ok (every listed failure fixed and re-run green), summary, files, unresolved [{item, reason}]."
  ].join("\n");
}

function parityPrompt(pairs) {
  return [
    `You compare screenshots of the same scenario in the Tauri app (reference) and the GPUI port, for epic ${epic.id} "${epic.title}". Read-only.`,
    `Scenario meanings: ${plan}/epics.json, key "scenarios". Decisions that explain intended differences: ${plan}/README.md, "Decyzje" (D8: GIF and video are placeholders in the GPUI lightbox; D14: window chrome).`,
    "Open both PNG files of every pair with the Read tool:",
    json(pairs),
    "Report the differences a user would notice: missing or extra elements, order, alignment, a wrong colour token, spacing off by more than a few pixels, wrong text or locale, wrong state (selection, focus, disabled).",
    "Ignore font rasterisation and antialiasing, sub-pixel offsets, the content of media that D8 replaces with a placeholder, and anything outside this epic's scope (name the epic that owns it in `where` and mark it minor).",
    "Severity: blocking = a control or content of this epic is missing or unusable; notable = clearly visible layout, colour or text difference; minor = everything else.",
    "Return diffs [{scenario, severity, where, expected, actual}]; an empty list when the pair matches."
  ].join("\n");
}

function parityFixPrompt(diffs, round) {
  return [
    header(),
    "",
    `Fix visual differences between the GPUI port and the Tauri reference (parity round ${round}). The Tauri screenshot is the reference; theme tokens come from src/styles.css through the client crate.`,
    json(diffs),
    `Screenshots are under artifacts/gpui-compare/<scenario>/ (tauri.png, gpui.png, side-by-side.png); open them with the Read tool.`,
    "Fix the GPUI side only. If a difference is intended by the plan's decisions, do not change code and list it under unresolved with the decision number.",
    "",
    RULES,
    "",
    "Return ok, summary, files, unresolved [{item, reason}]."
  ].join("\n");
}

function scopePrompt() {
  return [
    "Collect the review scope. Do not edit anything.",
    `Run exactly: \`python3 .claude/skills/review-changes/scripts/collect_scope.py --base ${base}\``,
    "From its JSON output return: ok=true, empty, base, target, diff_command, untracked (the paths whose status is \"A?\"), files (path, category and hot_path of every file), changed_lines (totals.changed_lines), app_source_lines (totals.app_source_lines), suggested_lenses, rules (the rule paths), docs.",
    "If the command fails return ok=false, empty=false and error with its stderr."
  ].join("\n");
}

function brief(scope) {
  const docs = [...new Set([...(scope.docs ?? []), `${plan}/README.md`, epicFile, ...(epic.docs ?? [])])];
  return [
    "Repository root: the current working directory. Read-only review; do not edit, build or run tests.",
    `Intent: GPUI port epic ${epic.id} "${epic.title}". A second UI adapter (workspace src-gpui/: crates media_tagger_client and media_tagger_gpui) is built next to the Tauri/Solid app, which must keep its behaviour and wire contract. Binding decisions and agent rules: ${plan}/README.md; epic scope and acceptance criteria: ${epicFile}.`,
    `Scope: worktree, base ${(scope.base ?? base ?? "").slice(0, 12)}, target working tree; ${(scope.files ?? []).length} files.`,
    `Diff: \`${scope.diff_command ?? "git diff -M " + base}\` (plus untracked: ${names(scope.untracked)})`,
    "Your files: all",
    `Rules to read: ${names(scope.rules)}   Docs to read: ${names(docs)}`,
    "User concerns: none",
    "",
    "Return your findings through the structured output: one entry per finding with the fields of .claude/skills/review-changes/references/signal-bar.md, and `notes` for the short prose summary. An empty findings list is a normal, valuable result."
  ].join("\n");
}

function verifyPrompt(scope, candidate) {
  return [
    brief(scope).split("\n").slice(0, 7).join("\n"),
    "",
    "Candidate finding, exactly as its reviewer wrote it:",
    json(candidate),
    "",
    "Return your verdict through the structured output (candidate = its id). Use empty strings for missing_fact and new_issue when they do not apply."
  ].join("\n");
}

function sideVerifyPrompt(scope, issue) {
  return [
    brief(scope).split("\n").slice(0, 7).join("\n"),
    "",
    "Candidate finding. Another verifier noticed it in passing and did not trace it, so its fields are incomplete:",
    issue,
    "",
    "Apply your method. If you confirm it, fill path, line, title, priority, origin, trigger, impact and fix yourself."
  ].join("\n");
}

function reviewFixPrompt(findings, round) {
  return [
    header(),
    "",
    `Fix the confirmed review findings below (review round ${round} of at most ${MAX_REVIEW}). Each was raised by a specialist reviewer and confirmed by an independent verifier.`,
    json(findings),
    "",
    "For each finding: check it against the current code, apply the smallest correct fix (the `fix` field is a suggestion), and add a regression test when the defect is observable at a test layer. If you conclude a finding is wrong, do not change code for it: return it under disputed with your evidence. Then run the targeted tests of the code you touched.",
    "",
    RULES,
    "",
    "Return ok, summary, files, fixed [{id, summary}], disputed [{id, reason}]. Every finding id appears in exactly one of the two lists."
  ].join("\n");
}

function gateFixPrompt(result, round) {
  return [
    header(),
    "",
    `\`bun run test:all\` failed after this epic (gate fix round ${round}). Failures:`,
    json(result.failures ?? []),
    `Full log: ${result.log ?? "docs/review/gpui-port/" + epic.id + "/jobs/test-all.log"}`,
    "",
    "First decide whether this epic caused each failure: compare with the epic's diff and read the failing test. caused = the diff broke it: fix the root cause (same rules as for check failures: no weakened assertions, no #[ignore], no deleted tests). unrelated = the failing test and the code it exercises are untouched by this epic, or it is a known flaky desktop case: change nothing and say why with evidence. unclear = you could not tell.",
    "",
    RULES,
    "",
    "Return ok (true when you fixed something or nothing needed fixing), classification, summary, files."
  ].join("\n");
}

function recordPrompt(state) {
  return [
    `Record the result of GPUI port epic ${epic.id} "${epic.title}". Result data:`,
    json(state),
    "",
    "Do exactly this, nothing else:",
    `1. Append one entry to the end of ${epicFile} (section '## Notatki agentów'), in Polish, in this shape:`,
    `   ### ${date} — workflow gpui-port-epic`,
    "   - Zrobione: ... (from implement.summary, tests.summary, decisions)",
    "   - Odkryte: ... (decisions, deferred scope, skipped test cases with reasons, parity differences left open, anything a later epic must know; path:line where the data has it)",
    "   - Zostało: ... (blocked reason, unresolved failures, open review findings with the report path, gate result)",
    "   - Testy: one line per check and for test:all, with pass/fail",
    "   Never write an absolute path that contains a home directory or a user name.",
    `2. If review.open, review.disputed, review.preExisting or review.uncertain has entries: write ${plan}/reports/${epic.id}-review-open.md in English. Start with the epic, the date, the review base and the number of rounds; then a table (id, priority, path:line, title, status) and, per finding, trigger, impact, suggested fix and evidence. Statuses: fixed-unreviewed (fixed after the last round, not re-reviewed), disputed (the fixer disagreed, with its reason), pre-existing, needs-runtime-check (with the missing fact). If none of the four lists has entries, do not create or delete any report.`,
    `3. Run \`python3 ${S}/port_state.py set-status ${epic.id} "${state.status}" --role workflow --note "<one sentence in Polish: the outcome and, when blocked, the reason>"\`.`,
    "4. Run `git diff --check` on the files you wrote and fix whitespace errors you introduced.",
    "Do not commit, stage or change any other file. Return ok and report (the report path, or an empty string)."
  ].join("\n");
}

const keyOf = (finding) =>
  `${finding.path}|${String(finding.title ?? "").toLowerCase().replace(/[^a-z0-9]+/g, " ").trim().slice(0, 80)}`;

function pickLenses(scope, round, lastByLens) {
  const files = scope.files ?? [];
  const paths = files.map((file) => file.path);
  const any = (pattern) => paths.some((path) => pattern.test(path));
  const all = [{ agent: "review-conventions", role: "lens_rules" }];
  if (any(/^(src-tauri|src-gpui)\//)) all.push({ agent: "review-rust", role: "lens" });
  if (any(/^src-gpui\//)) all.push({ agent: "review-gpui", role: "lens" });
  if (any(/^(src|e2e)\//)) all.push({ agent: "review-frontend", role: "lens" });
  if ((scope.suggested_lenses ?? []).includes("review-performance") || files.some((file) => file.hot_path)) {
    all.push({ agent: "review-performance", role: "lens" });
  }
  if (hard) all.push({ agent: "review-stability", role: "lens" });
  if (any(/^(scripts|e2e|packaging|\.claude)\//) || paths.includes("package.json")) {
    all.push({ agent: "review-tooling", role: "lens_rules" });
  }
  if (round <= 2) return all;
  // Later rounds only re-review fixes: keep the core lenses and those that still found something last round.
  const kept = all.filter((lens) => CORE_LENSES.includes(lens.agent) || (lastByLens[lens.agent] ?? 0) > 0);
  const dropped = all.filter((lens) => !kept.includes(lens)).map((lens) => lens.agent);
  if (dropped.length) log(`Review round ${round}: lenses without findings last round are skipped: ${dropped.join(", ")}`);
  return kept;
}

async function runJob(job, label, phaseName, extraArgs, what) {
  const result = await ask("clerk", jobPrompt(job, extraArgs ?? "", what), CHECKED, { label, phase: phaseName });
  return result ?? { ok: false, status: "error", error: "the clerk agent returned nothing", failures: [] };
}

// check -> fix -> check ... ; maxFixes is the number of fix attempts.
async function stabilize(tag, maxFixes, phaseName) {
  let checked = null;
  for (let round = 1; round <= maxFixes + 1; round++) {
    checked = await runJob("checks", `checks ${tag} #${round}`, phaseName, "",
      "the checks compile Rust and can take several calls.");
    if (checked.ok) return { ok: true, fixes: round - 1, failures: [] };
    if (checked.status !== "failed") return { ok: false, fixes: round - 1, failures: [], error: checked.error ?? checked.status };
    if (round > maxFixes) break;
    log(`${epic.id} ${tag}: ${checked.failures?.length ?? 0} failing check(s), fix round ${round}/${maxFixes}`);
    await ask(fixerRole, fixPrompt(checked.failures, checked.log, round, "The epic's checks fail"), FIXED,
      { label: `fix checks ${tag} #${round}`, phase: phaseName });
  }
  return { ok: false, fixes: maxFixes, failures: checked?.failures ?? [], log: checked?.log };
}

async function parity(state) {
  if (!input.parityAvailable || !epic.parity?.length) {
    log(epic.parity?.length
      ? `${epic.id}: screenshot comparison skipped, the compare:ui harness does not exist yet (epic E01)`
      : `${epic.id}: no comparison scenarios`);
    return true;
  }
  const scenarioArgs = epic.parity.map((scenario) => ` --scenario ${scenario}`).join("");
  for (let round = 1; round <= MAX_PARITY + 1; round++) {
    const compared = await ask("clerk", [
      jobPrompt("compare", scenarioArgs, "the comparison builds and starts both apps."),
      `5. On success also return available=true and pairs: for each scenario of ${epic.parity.join(", ")} the paths artifacts/gpui-compare/<scenario>/tauri.png and gpui.png that exist. If start reports that compare:ui does not exist, return ok=false, available=false.`
    ].join("\n"), COMPARED, { label: `compare #${round}`, phase: "Stabilize" });
    if (!compared?.ok || !compared.pairs?.length) {
      state.parity.error = compared?.error ?? "comparison produced no screenshots";
      log(`${epic.id}: screenshot comparison failed: ${state.parity.error}`);
      return true;
    }
    const reviewed = await ask("parity_reviewer", parityPrompt(compared.pairs), PARITY, { label: `parity #${round}`, phase: "Stabilize" });
    const diffs = reviewed?.diffs ?? [];
    const actionable = diffs.filter((diff) => diff.severity !== "minor");
    state.parity.rounds = round;
    state.parity.minor = diffs.filter((diff) => diff.severity === "minor");
    state.parity.open = actionable;
    if (!actionable.length || round > MAX_PARITY) break;
    await ask(fixerRole, parityFixPrompt(actionable, round), FIXED, { label: `fix parity #${round}`, phase: "Stabilize" });
    const settled = await stabilize(`after parity ${round}`, 2, "Stabilize");
    if (!settled.ok) {
      state.blocked = "checks fail after parity fixes";
      state.checks.failures = settled.failures;
      return false;
    }
  }
  if (state.parity.open.length) log(`${epic.id}: ${state.parity.open.length} visual difference(s) left open after ${MAX_PARITY} parity round(s); recorded in the epic notes`);
  return true;
}

async function review(state) {
  const seen = new Set();
  let lastByLens = {};
  const review = state.review;
  for (let round = 1; round <= MAX_REVIEW; round++) {
    review.rounds = round;
    const scope = await ask("clerk", scopePrompt(), SCOPE, { label: `scope r${round}`, phase: "Review" });
    if (!scope?.ok) {
      review.error = `scope collection failed: ${scope?.error ?? "no result"}`;
      break;
    }
    if (scope.empty) {
      log(`${epic.id}: nothing to review (empty scope against ${String(base).slice(0, 12)})`);
      review.clean = true;
      break;
    }
    const lenses = pickLenses(scope, round, lastByLens);
    const reviewBrief = brief(scope);
    const results = await parallel(lenses.map((lens) => async () => {
      const result = await ask(lens.role, reviewBrief, FINDINGS,
        { agentType: lens.agent, label: `${lens.agent} r${round}`, phase: "Review" });
      return { lens: lens.agent, result };
    }));
    const candidates = [];
    const roundKeys = new Set();
    let repeated = 0;
    lenses.forEach((lens, index) => {
      const result = results[index]?.result;
      if (!result) {
        review.gaps.push(`round ${round}: ${lens.agent} returned nothing`);
        return;
      }
      for (const finding of result.findings ?? []) {
        if ((finding.confidence ?? 0) < 60 || !finding.trigger) continue;
        const key = keyOf(finding);
        if (seen.has(key)) {
          repeated += 1;
          continue;
        }
        if (roundKeys.has(key)) continue;
        roundKeys.add(key);
        candidates.push({ ...finding, lens: lens.agent, id: `r${round}-${finding.id}` });
      }
    });
    if (repeated) log(`Review round ${round}: ${repeated} finding(s) already rejected, disputed or recorded in an earlier round were dropped`);
    if (!candidates.length) {
      log(`${epic.id}: review round ${round} is clean`);
      review.clean = true;
      break;
    }
    const verdicts = await parallel(candidates.map((candidate) => async () => {
      const role = ["P0", "P1"].includes(candidate.priority) ? "verifier_high" : "verifier_low";
      return ask(role, verifyPrompt(scope, candidate), VERDICT,
        { agentType: "review-verifier", label: `verify ${candidate.id}`, phase: "Review" });
    }));
    const confirmed = [];
    const sideIssues = [];
    candidates.forEach((candidate, index) => {
      const verdict = verdicts[index];
      if (!verdict) {
        review.gaps.push(`round ${round}: verifier for ${candidate.id} returned nothing`);
        return;
      }
      if (verdict.new_issue) sideIssues.push(verdict.new_issue);
      const finding = {
        ...candidate, priority: verdict.priority ?? candidate.priority, origin: verdict.origin ?? candidate.origin,
        verifier: verdict.reasoning
      };
      if (verdict.verdict === "confirmed") {
        confirmed.push(finding);
      } else {
        seen.add(keyOf(candidate));
        if (verdict.verdict === "uncertain") review.uncertain.push({ ...finding, missing_fact: verdict.missing_fact ?? "" });
        else review.rejected += 1;
      }
    });
    // A verifier's side observation gets one fresh verifier and is never reported unverified.
    const sideVerdicts = await parallel(sideIssues.slice(0, 5).map((issue, index) => async () =>
      ask("verifier_high", sideVerifyPrompt(scope, issue), SIDE_VERDICT,
        { agentType: "review-verifier", label: `verify side issue r${round}-${index + 1}`, phase: "Review" })));
    if (sideIssues.length > 5) log(`Review round ${round}: ${sideIssues.length - 5} side observation(s) beyond the first 5 were not verified`);
    sideVerdicts.forEach((verdict, index) => {
      if (verdict?.verdict !== "confirmed" || !verdict.path || !verdict.title) return;
      const finding = {
        id: `r${round}-side-${index + 1}`, lens: "review-verifier", path: verdict.path, line: verdict.line, title: verdict.title,
        priority: verdict.priority ?? "P2", origin: verdict.origin ?? "introduced", trigger: verdict.trigger ?? "",
        impact: verdict.impact ?? "", fix: verdict.fix ?? "", verifier: verdict.reasoning
      };
      if (!seen.has(keyOf(finding))) confirmed.push(finding);
    });
    const inherited = confirmed.filter((finding) => finding.origin === "pre-existing");
    for (const finding of inherited) {
      seen.add(keyOf(finding));
      if (["P0", "P1"].includes(finding.priority)) review.preExisting.push(finding);
    }
    const introduced = confirmed.filter((finding) => finding.origin !== "pre-existing");
    lastByLens = {};
    for (const finding of introduced) lastByLens[finding.lens] = (lastByLens[finding.lens] ?? 0) + 1;
    log(`${epic.id}: review round ${round}: ${candidates.length} candidate(s), ${introduced.length} confirmed, ${inherited.length} pre-existing`);
    if (!introduced.length) {
      review.clean = true;
      break;
    }
    review.confirmed += introduced.length;
    const fixed = await ask(fixerRole, reviewFixPrompt(introduced, round), REVIEW_FIXED,
      { label: `fix review r${round}`, phase: "Review" });
    const disputed = new Map((fixed?.disputed ?? []).map((item) => [item.id, item.reason]));
    const fixedIds = new Set((fixed?.fixed ?? []).map((item) => item.id));
    for (const finding of introduced) {
      if (disputed.has(finding.id)) {
        seen.add(keyOf(finding));
        if (REPORTED.includes(finding.priority)) review.disputed.push({ ...finding, status: "disputed", reason: disputed.get(finding.id) });
      } else if (fixedIds.has(finding.id) || DRY) {
        review.fixed += 1;
      }
    }
    const settled = await stabilize(`after review r${round}`, 2, "Review");
    if (!settled.ok) {
      state.blocked = `checks fail after the fixes of review round ${round}`;
      state.checks.failures = settled.failures;
      return false;
    }
    if (round === MAX_REVIEW) {
      // No round is left to re-review these fixes: report them instead of calling them done.
      const left = introduced.filter((finding) => !disputed.has(finding.id));
      review.open = left.filter((finding) => REPORTED.includes(finding.priority))
        .map((finding) => ({ ...finding, status: fixedIds.has(finding.id) ? "fixed-unreviewed" : "open" }));
      review.p3NotReported = left.length - review.open.length;
      log(`${epic.id}: ${MAX_REVIEW} review rounds used; ${review.open.length} P0-P2 finding(s) go to the report, ${review.p3NotReported} P3 not reported`);
    }
  }
  return true;
}

async function gate(state) {
  let unrelated = 0;
  for (let round = 1; round <= MAX_GATE + 1; round++) {
    state.gate.rounds = round;
    const result = await runJob("test-all", `test:all #${round}`, "Gate", "",
      "the full suite builds the apps and runs desktop E2E, so expect many calls.");
    if (result.ok) {
      state.gate.ok = true;
      state.gate.failures = [];
      return;
    }
    state.gate.ok = false;
    state.gate.failures = result.failures ?? [];
    if (result.status !== "failed") {
      state.gate.error = result.error ?? result.status;
      return;
    }
    if (round > MAX_GATE) return;
    log(`${epic.id}: test:all failed, gate fix round ${round}/${MAX_GATE}`);
    const fixed = await ask(fixerRole, gateFixPrompt(result, round), GATE_FIXED, { label: `fix gate #${round}`, phase: "Gate" });
    state.gate.fixes.push({ round, classification: fixed?.classification ?? "unclear", summary: fixed?.summary ?? "", files: fixed?.files ?? [] });
    if (fixed?.classification === "unrelated" && ++unrelated >= 2) {
      state.gate.error = "test:all keeps failing for reasons the fixer classified as unrelated to this epic";
      return;
    }
  }
}

async function main() {
  if (!epic.id || !base || !date) throw new Error("args need epic, base and date; build them with port_state.py args <EPIC>");
  const state = {
    epic: epic.id, title: epic.title, date, base, from: STEPS[startAt], status: "blocked", blocked: null, dryRun: DRY,
    implement: null, tests: null,
    checks: { ok: null, fixes: 0, failures: [] },
    parity: { rounds: 0, open: [], minor: [] },
    review: { rounds: 0, clean: false, confirmed: 0, fixed: 0, rejected: 0, open: [], disputed: [], preExisting: [], uncertain: [], gaps: [] },
    gate: { ok: null, rounds: 0, failures: [], fixes: [] }
  };
  const runs = (step) => !state.blocked && startAt <= STEPS.indexOf(step);

  if (runs("implement")) {
    phase("Implement");
    const result = await ask(hard ? "implementer_hard" : "implementer", implementPrompt(), IMPLEMENTED,
      { label: `implement ${epic.id}`, phase: "Implement" });
    state.implement = result;
    if (!result) state.blocked = "the implementer returned nothing";
    else if (result.blocked) state.blocked = `implementer blocked: ${result.blocked_reason ?? "no reason given"}`;
    else if (!result.ok) state.blocked = `implementer could not finish: ${result.summary}`;
  }
  if (runs("tests")) {
    phase("Tests");
    const result = await ask("test_author", testsPrompt(state.implement), TESTED, { label: `tests ${epic.id}`, phase: "Tests" });
    state.tests = result;
    if (!result?.ok) state.blocked = `the test author could not finish: ${result?.summary ?? "no result"}`;
  }
  if (runs("stabilize")) {
    phase("Stabilize");
    const settled = await stabilize("stabilize", MAX_FIX, "Stabilize");
    state.checks = { ok: settled.ok, fixes: settled.fixes, failures: settled.failures, error: settled.error };
    if (!settled.ok) state.blocked = settled.error ? `checks could not run: ${settled.error}` : `checks still fail after ${MAX_FIX} fix round(s)`;
    else await parity(state);
  }
  if (runs("review")) {
    phase("Review");
    await review(state);
  }
  if (runs("gate")) {
    phase("Gate");
    await gate(state);
    if (state.gate.ok !== true) state.blocked = state.gate.error ?? `test:all still fails after ${MAX_GATE} fix round(s)`;
  }
  const openP0 = [...state.review.open, ...state.review.disputed].some((finding) => finding.priority === "P0");
  if (!state.blocked && openP0) state.blocked = "an open P0 review finding remains";
  state.status = state.blocked ? "blocked" : "done";

  phase("Record");
  if (DRY) {
    await ask("scribe", "", RECORDED, { label: "record", phase: "Record" });
    log(`gpui-port-epic dry run finished: ${JSON.stringify({ epic: epic.id, status: state.status, review: state.review.rounds })}`);
    return state;
  }
  const recorded = await ask("scribe", recordPrompt(state), RECORDED, { label: `record ${epic.id}`, phase: "Record" });
  state.recorded = recorded?.ok === true;
  state.report = recorded?.report ?? "";
  const summary = {
    epic: epic.id, status: state.status, blocked: state.blocked, recorded: state.recorded, report: state.report,
    checks: { ok: state.checks.ok, fixes: state.checks.fixes },
    parity: { rounds: state.parity.rounds, open: state.parity.open.length },
    review: {
      rounds: state.review.rounds, clean: state.review.clean, confirmed: state.review.confirmed, fixed: state.review.fixed,
      open: state.review.open.length, disputed: state.review.disputed.length, preExisting: state.review.preExisting.length,
      uncertain: state.review.uncertain.length, gaps: state.review.gaps
    },
    gate: { ok: state.gate.ok, rounds: state.gate.rounds, fixes: state.gate.fixes.length }
  };
  log(`gpui-port-epic summary: ${JSON.stringify(summary)}`);
  return summary;
}

return await main();
