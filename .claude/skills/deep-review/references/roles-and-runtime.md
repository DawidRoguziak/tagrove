# Runtime proof

Each lens's review heuristics live in `.claude/agents/review-*.md`, and the bar lives in
`../review-changes/references/signal-bar.md`. This page covers only the runtime owner: the
single general-purpose agent, or the coordinator itself, that executes code to settle a
verdict.

## When to run

Run code only when a verdict depends on a runtime fact the code can't show, such as event
order, WebKit behavior, actual SQL plans, or a payload Rust really accepts. Pick the lowest
layer from the [test matrix](../../../../docs/development/testing.md#choosing-the-test-level)
and inspect the current package scripts first.

- Use focused Vitest, Rust unit and integration tests, and desktop E2E.
- New probes stay inside the run directory and use synthetic fixtures. Never change app code
  to make a check pass.
- Environment failures are gaps, not reproductions of a defect.

## Serialization

`run.json.runtime_owner` names the one owner. It serializes every desktop session and shared
frontend or E2E build, because these share `dist/`, `src-tauri/target-e2e` and ports
4444/4446/4447. Other agents request runtime evidence through the coordinator. Never kill
processes by name, and never delete a lock just because a client can't connect.

## Desktop checks

1. Use [verify-mediatagger](../../verify-mediatagger/SKILL.md) and its feature recipes.
2. Start with `bun run app:control start`, keep the foreground session alive, wait for
   `READY`, then run `doctor`. The controller supplies synthetic media, temporary XDG
   directories and a private display.
3. Before interacting, record the session identity, title, app-data root and display. Never
   attach to an installed app, a production-profile binary or production data.

Controller limits that shape a proof:

- one command at a time through one socket (150 s budget); `wait` polls for at most 90 s;
- script calls (doctor probes, `invoke`) time out after 30 s; click/fill wait 15 s for the
  element; a reload waits 20 s for the search input;
- `inspect` returns at most 20,000 characters of text and 200 elements; `logs` the last
  24,000 characters per log; the console buffer holds 1,000 entries of up to 16,000 characters;
- the doctor reports `alert-open`, `busy-maintenance` (an unbounded maintenance gate or
  library lock is held) or `tainted` (after native input); each state limits the commands
  that run, as listed in the verification skill;
- Xvfb has no window manager: maximize, minimize and minimum window size cannot be proven.

If you need to test dev-mode behavior itself, use `bun run tauri:dev` with fresh temporary
`XDG_DATA_HOME`, `XDG_CONFIG_HOME`, `XDG_CACHE_HOME` and `XDG_RUNTIME_DIR`, synthetic media and
a private display. Read the profile isolation section of
[system-overview.md](../../../../docs/architecture/system-overview.md) first.

## Evidence and cleanup

- Record each command with its working directory, exit code and the specific observation in
  the verification file.
- Look at screenshots with the image viewer. A capture command that succeeded doesn't prove
  any pixels rendered.
- Check persisted effects by reloading or by reading the temporary database.
- Known limits stay explicit gaps: native dialogs, MP4 pixels under Xvfb, and missing drivers.
- When done, stop the owned session and confirm that the processes exited and the temporary
  data is gone. Copy sanitized evidence into `verification/` and link any originals left in
  the ignored `artifacts/app-control/`.
