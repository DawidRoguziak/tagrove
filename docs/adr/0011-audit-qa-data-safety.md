# 0011: Audit QA runs on a reflink copy inside a sandbox

- Status: Accepted
- Date: 2026-11-08
- Areas: testing, data safety, security surface, agent tooling
- Related: [audit process](../development/audit/README.md), [audit safety model](../../.claude/skills/audit/references/safety.md), [testing: persistent desktop control](../development/testing.md#persistent-desktop-control)

## Context

The `/audit` QA stages click through the real E2E build against a large private dataset (about 30,000 images and long videos, `$AUDIT_DATASET`) to find bugs that only show at that scale. The app deletes, renames, imports, clears and rescans; the agents that drive it are language models that can make mistakes. The dataset must never change, the production and dev profiles must never be touched, and nothing private may reach the public repository.

`e2e/control.js` already isolates a session: E2E identifier and title, fresh XDG directories below an owned scratch root, a private authenticated Xvfb, owned process groups, and a doctor that refuses any scan root other than `<scratch>/media`. Its scratch root is created under `realpath(os.tmpdir())` and removed recursively on stop.

## Options considered

1. **An external scan root in `control.js`** (a command to add the dataset folder itself as a root). Simplest, and no copy. Rejected: destructive features (delete, duplicate resolver, Clear library, CSV/backup import) would act on the real files, and the doctor's single-root guard would have to be weakened.
2. **Bind-mount the dataset into the scratch root.** No copy cost. Rejected: `removeOwnedScratch` deletes the scratch root recursively, so a bind mount inside it turns `stop` into deleting the dataset; a read-only bind breaks every destructive scenario instead of testing it.
3. **A reflink copy inside the scratch root, with the session in a sandbox.** `TMPDIR` points into `$AUDIT_DATASET/for-claude-tests/q`, on the same btrfs filesystem, so `cp -a --reflink=always` clones the selected folders in seconds without using space; `stop` deletes only the clone. Chosen.

## Decision

- QA sessions start through `qa_session.py` with `TMPDIR=$AUDIT_DATASET/for-claude-tests/q`, inside `bwrap` (the home directory and the dataset read-only; only the repository, `for-claude-tests` and the build caches writable below them; production and dev profile folders replaced by tmpfs) and `dbus-run-session` with a private `XDG_RUNTIME_DIR`. `qa_fs.py copy` reflinks the block's folders into `<scratch>/media/dataset/`; the app's only root stays `<scratch>/media`.
- `control.js` guards are unchanged. It gains input commands (modifier clicks, drag, wheel, select, wait, resize, alerts, XTest native input through `e2e/control-native.py`), a socket-path length check, and doctor states (`alert-open`, `busy-maintenance`, `tainted`) that restrict commands instead of weakening checks.
- QA agents act only through `app:control`, `qa_fs.py` (re-validated, then re-executed in bwrap with everything read-only except the session copy) and `audit_state.py put`. A PreToolUse allowlist hook (`qa_hook.py`) enforces it in two layers: agent frontmatter and the ignored `settings.local.json`, filtered on `agent_type` `qa-*`.
- `dataset_guard.py` compares the dataset with a manifest after init, before each copy, after destructive suites and after every stop; a monitor stops the session on any write descriptor under the dataset, a production profile, or the home directory outside the repository and build caches; any failure halts the run until a human clears it.
- The dataset path lives only in the ignored `docs/review/<run>/run.json`; reports live in the ignored `docs/review/`.

## Consequences

- Destructive scenarios run against real-scale data at no disk cost, and a mistake costs one clone.
- The dataset must be on a reflink-capable filesystem with a writable `for-claude-tests/` and a path short enough for the 107-byte socket limit; `audit_init.py` checks all three.
- Native GTK dialogs work inside the sandbox only with `GTK_USE_PORTAL=0`; `audit_init.py` smoke-tests a file dialog under bwrap. The alert path is proven only in the first QA session.
- Whether frontmatter hooks fire for workflow agents is undocumented; the settings layer covers it. `audit_state.py pending` refuses QA stages unless both layers are installed.
- The maintenance gate and the library-wide locks wait without a limit, so the doctor reports `busy-maintenance` during backup import/export instead of failing; that unbounded wait itself is an audit candidate, not a harness change.
