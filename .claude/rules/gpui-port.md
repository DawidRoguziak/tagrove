---
paths:
  - "docs/plans/gpui-port/**"
  - ".claude/workflows/gpui-port-epic.js"
  - ".claude/skills/gpui-port/**"
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
- Epic E00 adds the `src-gpui/**` globs to this rule once the workspace exists (every glob here must match a file).
- After changing the scripts or the workflow: `python3 -m unittest discover -s .claude/skills/gpui-port/scripts`.
