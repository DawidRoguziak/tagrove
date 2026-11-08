# /audit safety model

The QA stages drive the real app against a large private dataset. The app must only ever see a
disposable copy, and no agent may change anything else. Several independent layers enforce
that; each one alone would be enough for the common mistakes.

## Data

- `$AUDIT_DATASET` is passed once to `audit_init.py --dataset` and stored only in the ignored
  `docs/review/<run>/run.json`. It never appears in tracked files, reports or prompts beyond the
  QA briefs that need the copy path.
- Only `$AUDIT_DATASET/for-claude-tests/` is writable. QA sessions use
  `TMPDIR=$AUDIT_DATASET/for-claude-tests/q`, so the controller's scratch root (and its socket,
  checked against the 107-byte limit) lives there, on the same btrfs filesystem as the dataset.
- After `READY`, `qa_fs.py copy --block <B>` clones the block's folders with
  `cp -a --reflink=always` into `<scratch>/media/dataset/`. The app is told to rescan; its only
  scan root is `<scratch>/media`. `stop` deletes the scratch root and so the copy.
- Never bind-mount anything inside the scratch root: `removeOwnedScratch` deletes it
  recursively.

## Process isolation

`qa_session.py start` runs, in a new session (setsid semantics) and returning immediately:

```
bwrap --dev-bind / / --ro-bind ~ ~ --ro-bind $AUDIT_DATASET $AUDIT_DATASET
      --bind ~/.cargo ~/.cargo --bind ~/.bun ~/.bun --bind ~/.cache ~/.cache   (those that exist)
      --bind <repo> <repo> --bind $AUDIT_DATASET/for-claude-tests $AUDIT_DATASET/for-claude-tests
      --tmpfs ~/.{local/share,config,cache}/com.example.mediatagger{,.dev}   (those that exist)
  -- dbus-run-session -- env XDG_RUNTIME_DIR=<private 0700> GSETTINGS_BACKEND=memory
     GTK_USE_PORTAL=0 NO_AT_BRIDGE=1 MEDIATAGGER_PERF=1 TMPDIR=<qa tmpdir> [VITE_MEDIATAGGER_PERF=1]
     bun run app:control start
```

`DISPLAY`, `WAYLAND_DISPLAY`, `XAUTHORITY`, `DBUS_SESSION_BUS_ADDRESS` and `XDG_RUNTIME_DIR`
are removed from the inherited environment. The whole home directory is read-only, so a native
file dialog steered to `~/Pictures` or `~/.bashrc` cannot write there; the build caches stay
writable because the controller rebuilds inside the sandbox. `/tmp` and devices are as on the
host. The controller itself adds its guards: E2E
identifier and title, app data below its scratch root, only `<scratch>/media` as a root,
owned process groups, an authenticated private Xvfb, and a native-input helper that refuses
any display or scratch that is not the session's.

`qa_session.py prebuild [--perf]` builds outside the sandbox with the same
`VITE_MEDIATAGGER_PERF` the session will use (the flag is compiled into the bundle), and
`start` refuses when the last build's flag differs.

## Agents

- `qa-tester` and `qa-refuter` have no Edit/Write tools.
- Their PreToolUse hook (`scripts/qa_hook.py`) allows Bash only from an allowlist:
  `app:control` without start/stop, `qa_fs.py`, `qa_session.py mark|maintenance|guard|wait`,
  `audit_state.py put|pending|models` (quoted heredoc only), `sqlite3 -readonly`, read-only text
  tools and git. It blocks command substitution, environment assignments, redirection to
  files, write-capable options, and any reference to the dataset outside `for-claude-tests` or
  to the production/dev profile folders, in every tool's input. Tools other than Bash, Read,
  Grep, Glob, LS, StructuredOutput, TodoWrite and ToolSearch are refused (default deny), and
  `WebFetch`/`WebSearch` are also in the agents' `disallowedTools`.
- The hook is installed twice: in the agents' frontmatter (`--scope agent`), and in the ignored
  `.claude/settings.local.json` for every tool call, filtered to inputs whose `agent_type`
  starts with `qa-` (`--scope settings`). The second layer exists because it is not documented
  whether frontmatter hooks fire for `agent({agentType})` calls inside a workflow. While a QA
  session is active it also refuses non-read tools for any subagent whose `agent_type` the
  harness did not report.
  `audit_state.py pending` refuses QA stages unless both layers are installed.
- `qa_fs.py` re-validates every target inside the copy, then re-executes itself in bwrap with
  the whole filesystem read-only except the session's media directory.

## Guards and halting

- `dataset_guard.py` compares the dataset (outside `for-claude-tests/`) with the init manifest:
  path, type, size, mtime, ctime, inode and mode, plus leftover `.mediatagger-*.pending` and
  `.*.tmp` files. It runs after init, before each copy, after every destructive suite and after
  every stop.
- The session monitor samples RSS and CPU of the session's process tree every 2 s
  (`qa/sessions/*-samples.csv`) and stops the session if any process holds a file open for
  writing under the dataset (outside `for-claude-tests/`), under a production profile, or under
  `$HOME` outside the repository and the build caches.
- A watchdog stops a session after 45 minutes without a new `actions.jsonl` line, unless
  `qa_session.py maintenance --minutes N` (N ≤ 120) covers a long backup export/import.
- Any guard failure, failed stop or monitor violation sets `run.json.halted`. Scripts set it;
  agents cannot. Every later `pending`, `put` and session start refuses until the user
  inspects the run and runs `clear_halt.py --confirm-inspected`, a separate script that is in no
  permission allowlist.
- `audit_state.py put` replaces the dataset path with `$AUDIT_DATASET` and the home directory
  with `~` in everything it stores.

## Session lifecycle

`reap` → `prebuild` → `start` → `wait` (repeat until ready) → `copy` → guard → suites (guard
after each destructive one) → `stop` (always, in `finally`) → guard. `stop` first restores
`u+rwX` in the copy (suites may `chmod 000`), then calls `app:control stop <id>` with the run's
`TMPDIR` and verifies that the manifest says stopped, the scratch root is gone, the controller
exited and no lock remains. Every stage starts with `reap`. A controller that died without
cleanup halts the run; recovery is manual (verify-mediatagger, Cleanup). Nothing ever kills
processes by name or deletes a lock because a client could not connect.
