# Staged multi-agent audit

Implementation entry points: [`/audit` skill](../../../.claude/skills/audit/SKILL.md), [finding format](../../../.claude/skills/audit/references/finding-format.md), [safety model](../../../.claude/skills/audit/references/safety.md), [QA driving notes](../../../.claude/skills/audit/references/qa-driving.md), [known sources](../../../.claude/skills/audit/references/known-sources.md), [models](../../../.claude/skills/audit/references/models.json), [static batches](../../../.claude/skills/audit/references/batches.json), [QA blocks](../../../.claude/skills/audit/references/qa-blocks.json), [recon candidates](../../../.claude/skills/audit/references/recon-candidates.json), [scripts](../../../.claude/skills/audit/scripts/). Scenario catalog: [qa-scenarios.md](qa-scenarios.md). Safety decision: [ADR 0011](../../adr/0011-audit-qa-data-safety.md).

## Purpose

The audit looks for bugs, performance problems, and stability problems in the whole project, in two phases:

1. **Static.** Specialist agents read the code in batches. Every candidate finding gets exactly one refuter and one judge.
2. **Real-app QA.** Tester agents click through the real E2E build of Tagrove on a private Xvfb display, against a disposable copy of a large private dataset, while watching the console and logs. Every bug gets exactly one refuter (which tries to reproduce it) and one judge.

A third pass assembles a report with coverage and gaps.

Results live in `docs/review/<run>/`, which is git-ignored (`/docs/review/`). Run directories can contain a private dataset path, logs, and screenshots, so never commit them. The process itself (skill, scripts, agents, workflows, these docs) is tracked. The audit is separate from `/review-changes` and `/deep-review`, which review a diff; the audit reviews the whole tree and drives the app. It reuses the `review-rust`, `review-frontend`, `review-performance`, `review-conventions`, and `review-verifier` agents and adds `review-stability`, `review-tooling`, `audit-judge`, `audit-clerk`, `qa-tester`, and `qa-refuter`.

Each stage runs one saved workflow (`.claude/workflows/audit-<stage>.js`); `/audit` chains all of them. The coordinator runs scripts and the workflow and reads only counters and `STATUS.md`; agents do the reading and clicking. State is on disk, so every stage is resumable.

## Stages

| Stage | Command | What runs | Output under `docs/review/<run>/` |
| --- | --- | --- | --- |
| init | `/audit init --dataset <path>` | Script only (`audit_init.py`): run directory, baseline HEAD and dirty-state hashes, tool inventory, dataset manifest, hook-layer check, smoke tests (bwrap plus GTK file dialog on a private Xvfb, reflink across the sandbox binds) | `run.json`, `STATUS.md`, `context/` (copies of batches, models, QA blocks, recon candidates; tool inventory; `dataset-manifest.json`), `guard/` |
| static | `/audit static [--include-optional]` | `run_tools.sh` through a clerk (`bg.py`), in parallel one finder agent per batch and lens, then per-batch dedup by a clerk. Followed by `assign-ids --kind static` (adds the recon candidates once every required finder is stored) and `dup-index` | `tools/`, `raw/`, `records/S-*.json` (state `candidate`), `dup-index.json`, `static-findings.md` |
| static-verify | `/audit static-verify` | Per record: one refuter (`review-verifier`), then one judge (`audit-judge`). Followed by `dup-index` | `records/S-*.json` (state `judged`), `static-findings.md` |
| qa-prep | `/audit qa-prep` | In parallel: E2E sweep (`run_e2e_sweep.py` through a clerk, serial, one fresh display and XDG root per spec; each spec item also gets a private GTK `settings.ini` and GSettings keyfile in that root, animations off for `+reduced-motion` and on otherwise, plus `MEDIATAGGER_NATIVE_DISPLAY` from the `xvfb-run -a` display, as in testing.md) and the QA planner (read-only) | `qa/sweep/`, `qa/plan.json`; failed sweep items become QA records in `qa-verify` |
| qa-run | `/audit qa-run [S1\|S2\|S3\|S4]` | Per block, in order: reap, prebuild, session start and wait, dataset copy and guard (clerk), one `qa-tester` per suite, guard after every destructive suite, triage; `stop` and guard always run. Followed by `qa_session.py reap` | `qa/runs/<block>/<suite>.json`, `triage.json`, `qa/sessions/`, `qa/fs-log.jsonl`, `guard/`; screenshots and logs in `artifacts/app-control/<session>/` |
| qa-verify | `/audit qa-verify` | Narrative dedup first (a dedup-only pass, then `assign-ids --kind qa`, which creates `records/Q-*.json` from suite bugs, triage candidates and failed sweep items, and a second pass), then one `qa-refuter` per bug (reproduction is serialized through sessions, destructive ones in a fresh session on a subset), then one judge | `records/Q-*.json` (judged), `qa-findings.md` |
| report | `/audit report` | `coverage.py` and `dup-index` first, then the report editor | `coverage.json`, `coverage.md`, `report.md` |
| status | `/audit status` | `audit_state.py status` | regenerates `STATUS.md`, `static-findings.md`, `qa-findings.md` |

Rules that hold for the verification stages:

- **One refuter and one judge per finding. There are no further rounds.** A record in state `refuted` skips the refuter on a resume, so it never gets a second one.
- A refuter or judge that notices a new problem does not extend the chain. The issue is stored as an **unverified lead** (`refutation.new_issue`, or `leads` in a finder result) and listed in the report.
- **Recon candidates** (`recon-candidates.json`, `origin: recon`) are observations from the planning reconnaissance. The static stage adds them after the finders have run, so finders stay independent. They go through the same single refuter and judge. Line numbers are hints; refuters re-read the current code.
- The judge's static verdicts are `confirmed`, `rejected`, `duplicate`, `known`, and `unproven`. `known` needs a `known_source` (see known-sources.md). `unproven` needs a `missing_fact`: it means a runtime fact is missing, it is final for statics, and it **feeds the QA planner**, which turns such records (and confirmed findings visible in the UI) into targeted `QA-T*` suites. QA verdicts are `confirmed`, `rejected`, `not-reproducible`, `duplicate`, and `known`.
- A **dispute** is a record where the refuter answered `uncertain`, or `rejected` a P0/P1 finding, or proposed a priority at least two levels away from the finder's (`is_dispute` in `audit_state.py`). Disputes and P0/P1 records get the strongest judge (`judge_high`).
- Judges open the decisive evidence of both sides themselves and do not run a new investigation. Neither refuter nor judge sees the finder's notes beyond the record.
- Static batches are in `batches.json` (26 batches; the optional Rust integration-test batches B15a and B15b run only with `--include-optional`). Test files are excluded unless a batch sets `tests`.
- QA blocks are in `qa-blocks.json` and run strictly one after another: S1 scale (QA-01..09, QA-09 needs a separate `VITE_MEDIATAGGER_PERF=1` build), S2 mutations and data safety (QA-10..14, all destructive, edge-file creation allowed), S3 media (QA-15), S4 keyboard, accessibility, window (QA-16), plus targeted `QA-T*` suites from the plan. A tool-runner failure or failing test is itself a candidate finding.

## Roles and models

Roles resolve through [`models.json`](../../../.claude/skills/audit/references/models.json). Workflows never hard-code a model: they read `args.models[role]`, produced by `audit_state.py pending`. Override for one invocation with `--models role=model[:effort],...`, for example `/audit static-verify --models judge_low=sonnet` or `--models finder=sonnet:medium`. `audit_state.py models [--role R] [--record ID]` shows the resolved choice. The `model:` in an agent's frontmatter is only a default; the role table wins.

| Role | Model | Effort | Used for | Approximate count |
| --- | --- | --- | --- | --- |
| `finder_critical` | fable | high | rust, frontend, perf, stability lenses in B3a, B3b, B4a, B4b, B5 | 15 |
| `finder` | opus | high | the same lenses in all other batches | 36 |
| `finder_rules` | sonnet | high | conventions and tooling lenses | 14 |
| `static_refuter_high` | fable | high | `review-verifier` for P0/P1 candidates | share of candidates |
| `static_refuter_low` | sonnet | high | `review-verifier` for P2/P3 candidates | rest of candidates |
| `judge_high` | fable | high | `audit-judge` for P0/P1 or a dispute (static and QA) | share of candidates |
| `judge_low` | opus | high | `audit-judge` otherwise | rest of candidates |
| `qa_planner` | fable | high | QA plan from the catalog plus targeted QA-T suites | 1 |
| `qa_tester` | sonnet | high | `qa-tester` in S1, S3, S4 | 11 base suites |
| `qa_tester_safety` | fable | high | `qa-tester` in S2 | 5 suites |
| `qa_refuter` | opus | high | `qa-refuter` outside S2 | one per QA bug |
| `qa_refuter_safety` | fable | high | `qa-refuter` for S2 and data-safety bugs | one per such bug |
| `qa_dedup` | sonnet | medium | narrative QA bug dedup | 1 per run |
| `clerk` | haiku | low | `audit-clerk`: tool runner, sweep, static dedup, session start/wait/stop, copy, guard, triage | many, cheap |
| `report_editor` | sonnet | medium | final report text | 1 |

### Agent count for a full run

- **Finders: 65** (without B15a/B15b): 15 on Fable (the 15 non-conventions lenses in the five critical batches), 36 on Opus (non-critical rust, frontend, perf, stability lenses), 14 on Sonnet (conventions and tooling lenses). With `--include-optional`, two more tooling finders on Sonnet.
- **Refuters and judges:** one of each per candidate record, so with `N` candidates after dedup there are `N` refuters and `N` judges. Refuters: P0/P1 candidates on Fable, the rest on Sonnet (QA: Opus, Fable for S2 and data-safety). Judges: Fable for P0/P1 and disputes, Opus for the rest. Expect roughly 10-20% of candidates on Fable refuters and 20-30% on Fable judges. Total verification agents = `2 * (N_static + N_qa)`. The number is not known before the finders finish.
- **QA:** 1 planner (Fable); 11 base Sonnet tester suites (S1 nine, S3 one, S4 one) and 5 Fable tester suites (S2), plus the targeted `QA-T*` suites the planner adds; narrative dedup on Sonnet; clerks on Haiku for the sweep, every session start/wait/copy/stop/guard, and triage; 1 report editor.

API list prices per 1M tokens (input/output): Fable 5.1 $10/$50, Opus 5.5 $4/$20, Sonnet 5.5 $2/$10, Haiku 5.5 $0.10/$0.50. All have a 1M context. The audit is expensive; the long sessions are the tester and refuter agents, and the finders on Fable.

## Safety summary

Full model: [safety.md](../../../.claude/skills/audit/references/safety.md). Decision and rejected alternatives: [ADR 0011](../../adr/0011-audit-qa-data-safety.md).

- **Copy only.** The dataset (`$AUDIT_DATASET`) is never an app scan root. Only `$AUDIT_DATASET/for-claude-tests/` is writable, and the session scratch root (`TMPDIR=$AUDIT_DATASET/for-claude-tests/q`) lives there. After `READY`, `qa_fs.py copy --block <B>` clones the block's folders into `<scratch>/media/dataset/` with `cp -a --reflink=always` (same btrfs filesystem, nearly free, fails instead of falling back to a real copy). The app's only root is `<scratch>/media`; `stop` deletes the scratch root and the copy.
- **Process isolation.** `qa_session.py start` runs the controller inside bubblewrap (`$HOME` and the dataset read-only; below them only the repository, `for-claude-tests` and the build caches `~/.cargo`, `~/.bun`, `~/.cache` writable; production and dev profile folders hidden by tmpfs) and `dbus-run-session`, with the host display, D-Bus, and XDG runtime variables removed. The controller adds its own guards (E2E identifier and title, owned process groups, authenticated private Xvfb).
- **Two hook layers** enforce one PreToolUse policy (`scripts/qa_hook.py`) for `qa-tester` and `qa-refuter`: no Edit/Write, Bash only from an allowlist, no path into the dataset outside `for-claude-tests` or into production/dev profile folders. One layer is the agents' frontmatter; the other is the ignored `.claude/settings.local.json`, filtered by `agent_type` starting with `qa-` (and failing closed for a subagent without a reported type while a QA session is active). Tools outside the QA allowlist (for example WebFetch, WebSearch, MCP tools) are refused. `audit_state.py pending` refuses QA stages unless both are installed.
- **File operations** by QA agents go only through `qa_fs.py`, which validates every target inside the copy and re-executes in bubblewrap with everything read-only except the session's media directory.
- **Guards.** `dataset_guard.py` compares the dataset outside `for-claude-tests/` with the init manifest (path, type, size, mtime, ctime, inode, mode, leftover `.mediatagger-*.pending` and `.*.tmp` files) after init, before each copy, after every destructive suite, and after every stop.
- **Monitor and watchdog.** A 2 s sampler records RSS and CPU and stops the session when any process holds a file open for writing under the dataset (outside `for-claude-tests/`), a production profile, or `$HOME` outside the repository and the build caches. A watchdog stops a session after 45 minutes without a new `actions.jsonl` line, unless `qa_session.py maintenance --minutes N` (N <= 120) covers a long backup export or import.
- **Halting.** A failed guard, failed stop, or monitor violation sets `run.json.halted`. Scripts set it, agents cannot. Every later `pending`, `put`, and session start refuses until a user inspects the run.
- The audit never edits application files, creates a worktree, commits, or kills processes by name, and never deletes a lock because a client cannot connect.

## Running and resuming

One command runs everything:

```sh
/audit --dataset <path>
```

The coordinator creates the run (`init`, including the smoke tests), runs `static` and
`static-verify`, checks the QA gate, runs `qa-prep`, `qa-run` (all blocks) and `qa-verify`, and
writes `report.md`, without asking anything. The dataset path is stored only in the ignored
`docs/review/<run>/run.json`; never write it into tracked files, reports or commit messages.

- **Retries.** After each stage the coordinator asks `audit_state.py pending` again and repeats
  the stage for items that failed (at most 3 times; 4 for `qa-verify`, which needs a dedup pass
  first). What still fails is recorded as a gap and the run continues.
- **QA gate.** QA runs only when the required tools are present, both hook layers are
  installed, the init smoke tests (`bwrap` with a GTK dialog, reflink) passed, and the automatic
  hook smoke passes: a `qa-tester` probe (workflow `audit-hook-smoke`) tries `touch` and
  `python3 -c` writes into the run folder, and `audit_state.py hook-check --evaluate` requires
  that qa_hook logged both blocks in `hook-log.jsonl`, that no probe file exists, and that the
  allowed `put hook-probe` went through. `pending` refuses `qa-run` and `qa-verify` until it
  passed. If the gate fails, QA is skipped and the report lists why.
- **Resuming.** `/audit` without `--new` resumes the latest unfinished run where it stopped;
  every step is idempotent and results are write-once. `--new` starts a new run.
  `--models role=model[:effort]` points roles at other models.
- **Scale.** Invoking `/audit` is the explicit request for full scale (about 65 finders in the
  static stage). Raising "Dynamic workflow size" in `/config` is optional; per-workflow
  concurrency is `min(16, CPUs - 2)`, 14 on a 16-CPU host.
- **Single stages** stay available for debugging: `/audit init|static|static-verify|qa-prep|qa-run [S1-S4]|qa-verify|report|status`.
- **Exclusive checkout.** QA stages share `dist/`, `src-tauri/target-e2e` and ports
  4444/4446/4447 with desktop E2E and `app:control`; do not run either in parallel
  ([testing.md](../testing.md)).
- **Halted run — the only stop.** If a guard finds a change in the dataset, a session does not
  stop cleanly, or a process writes outside the copy, the run halts and the coordinator stops.
  Inspect the dataset and the session leftovers, then run yourself:

  ```sh
  python3 .claude/skills/audit/scripts/clear_halt.py --confirm-inspected
  ```

  and start `/audit` again to resume. The script is a separate file outside every permission
  allowlist, so Claude Code always asks before it runs; agents never run it.

## Result layout

```text
docs/review/
  LATEST                      pointer to the newest run
  <run>/                      <yyyymmdd>-<hhmm>[-label]
    run.json                  private: dataset path, baseline, active_session, halted, models
    STATUS.md                 authoritative counters and the halt reason
    context/                  copies of batches, models, QA blocks used by this run
    tools/                    tool-runner logs and raw-results
    raw/                      write-once finder results (<batch>-<lens>.json) and per-batch dedup
    records/                  S-*.json and Q-*.json: finding, refutation, judgment, state
    dup-index.json            records grouped by file and nearby lines, for judges
    static-findings.md        generated view of static records
    qa/
      plan.json               QA plan (catalog suites plus QA-T*)
      sweep/                  one JSON and log per spec, summary.json
      runs/<block>/           suite reports, triage-raw.json, triage.json
      sessions/               per-session controller logs, monitor logs, RSS/CPU samples CSV
      dedup.json              QA bug dedup groups
      fs-log.jsonl            every qa_fs.py operation
    guard/                    dataset guard reports (the manifest is context/dataset-manifest.json)
    bg/                       state and output of the detached tool runner and E2E sweep
    hook-smoke/, hook-log.jsonl  automatic QA hook proof and the log of every hook block
    coverage.json, coverage.md  UI controls the testers never touched
    qa-findings.md            generated view of QA records
    report.md                 final report
```

Screenshots, `actions.jsonl`, `console.jsonl` and `process.log` of each session stay in the ignored `artifacts/app-control/<session>/`. The layout is defined by the scripts (`audit_lib.py`, `audit_state.py`); if this tree drifts, the scripts win.

## Limits and known gaps

- **Windowed gallery mode is unreachable at 30k items** (it starts beyond a 16,000,000 px scroll height), and so is the position scrollbar. QA records this as a gap; the behavior is covered by the `gallery-100k` sweep item and `e2e/gallery-scale`. Controls reachable only at that scale are flagged `scale_only` in the coverage report.
- **No window manager on Xvfb:** maximize, minimize, and minimum window size cannot be proven. `:focus-visible` needs a native click.
- **MP4 picture areas may be black** under Xvfb, so video checks rely on state, logs, and metadata rather than pixels.
- **Alert handling is not smoke-tested at init.** The `window.alert` path needs an app session, so it is recorded as deferred to the first QA session.
- **Frontmatter hooks for workflow agents are unverified.** It is undocumented whether `hooks:` in agent frontmatter fire for `agent({agentType})` calls inside a workflow. That is why the second layer in `.claude/settings.local.json` exists.
- **Workflow summaries are logs.** Each workflow logs its summary (the scripts keep no top-level `return`, so `node --check` can parse them), but the authoritative counters are in `STATUS.md`, regenerated by `audit_state.py status`.
- **The Bash tool has a 10-minute limit per call.** Long waits go through `qa_session.py wait` and `bg.py wait` (at most 540 s per call, repeated until done); sessions, the tool runner and the E2E sweep are started detached and return at once.
- **Static analysis cannot prove runtime facts.** Such findings end as `unproven` and become targeted QA suites; those left unverified appear as gaps in the report.
- **Not a replacement for tests.** Findings are evidence-backed claims, not fixes. The audit does not edit code; fixes follow the normal test-level rules in [testing.md](../testing.md).
