---
paths:
  - "docs/plans/gpui-port/**"
  - ".claude/workflows/gpui-port-epic.js"
  - ".claude/skills/gpui-port/**"
  - "src-gpui/Cargo.toml"
  - "src-gpui/crates/media_tagger_client/**"
  - "src-gpui/crates/media_tagger_gpui/**"
  - "scripts/gpui-lock-parity.py"
  - "scripts/gpui-build-e2e.mjs"
---

# GPUI port

Read `docs/plans/gpui-port/README.md` (decisions D1 to D15, agent rules, epic flow).

- The Tauri/Solid app keeps its behaviour and wire contract for the whole port. `bun run test:all` stays green after every epic.
- The port is its own Cargo workspace, `src-gpui/` (own `Cargo.lock` and target directories), with the core as a path dependency. `src-tauri/Cargo.toml` and `src-tauri/Cargo.lock` never gain a GPUI or git dependency.
- GPUI comes only from the `zed-industries/zed` repository at one pinned `rev` (decision D1). No `gpui-pre`, no `gpui-component`.
- Dependency direction: `media_tagger_gpui` → `media_tagger_client` → `media_tagger_core`. The client crate has no GUI dependency and reaches the core only through its `Backend` port.
- `docs/plans/gpui-port/epics.json` is the source of truth for epics. Change statuses only with `port_state.py set-status`, and run `port_state.py validate` and `render` after editing the manifest.
- The workflow `gpui-port-epic` never commits and never names a model: roles come from `.claude/skills/gpui-port/references/models.json`. Agents that write run one at a time; no worktrees.
- Long or display-bound commands go through `.claude/skills/gpui-port/scripts/job.py`. Check commands must pass `command_problem` in `port_state.py` (cargo with `--locked`, no shell syntax, no `test:all`).
- Run state lives in the ignored `docs/review/gpui-port/`. Tracked notes and reports carry no private paths.
- Cargo commands for the port take `--manifest-path src-gpui/Cargo.toml --locked`. `src-gpui/Cargo.lock` changes only with an intended dependency change; after a change to a core dependency run `bun run gpui:lock-parity`. A Zed rev bump follows `docs/development/setup-and-build.md` ("Updating the pinned Zed commit").
- Profiles are `com.example.mediatagger.gpui.dev` and `.gpui.e2e` only (`src-gpui/crates/media_tagger_gpui/src/profile.rs`); the identifier is baked in by `build.rs`. Never add a Tauri identifier or a release identifier to that list (cutover epic E22).
- Layer rules are enforced by `src-tauri/tests/architecture_boundaries.rs`: the client imports `media_tagger_core::{models, error}` only, outside `backend/core_backend.rs`; the adapter reaches the core only in `boot.rs` and never `db::`, lock helpers, `Connection` or `admit`. Architecture and tests: `docs/architecture/system-overview.md` ("GPUI port"), `docs/development/testing.md` ("GPUI workspace"), ADR 0036.
- Every glob in this rule must match a file.
- After changing the scripts or the workflow: `python3 -m unittest discover -s .claude/skills/gpui-port/scripts`.
