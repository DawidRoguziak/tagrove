# Roles and runtime checks

Choose perspectives appropriate to each bounded area. Two copies of the same assignment
are not two perspectives. All reviewers inspect callers, guards, contracts and tests beyond
the changed lines. A performance finding needs a mechanism and relevant scale; a design
risk needs a concrete change or failure scenario and consequence. Cosmetic preferences,
unsupported rewrites and missing tests alone are not findings.

- Frontend: trace user actions through components, state, hooks, services and IPC. Check
  async completion order, identity, cancellation, invalidation, virtualized rendering,
  keyboard/focus behavior, accessible semantics and locale placeholder parity.
- Backend: trace commands through services, workers, SQLite and filesystem resources.
  Check transactions, revision invalidation, lock order, queue bounds, partial failures,
  file identity, rollback, subprocess/native lifetimes and boundary validation.
- Architecture: trace producers and consumers across boundaries, including serialization,
  state ownership, persistence, shutdown, recovery, coupling and test assumptions.
- Discovery: map ownership, callers, relevant documentation and existing test entry points.
  Return facts and bounded follow-up areas. This is not a substantive review perspective.
- Verification: judge the original claim. Check reachability, validation and counterexamples.
  Confirm through executed evidence or a concrete source-and-contract trace. Runtime facts
  still uncertain remain unresolved. Environment failures are gaps, not defect reproductions.
  Preserve conflicting evidence. New issues go back to the coordinator for another verifier.

Read the repository [test matrix](../../../../docs/development/testing.md#test-layer-matrix)
and inspect current package scripts before execution. Focused frontend unit tests, Rust
unit/integration tests and desktop E2E are allowed. Select the lowest layer that proves
the claim. New probes stay in the ignored run folder with synthetic fixtures.

The coordinator names one runtime owner in `run.json`. That owner serializes desktop
sessions and all shared frontend/E2E builds, including builds requested by other agents.
Other agents request runtime evidence by message and do not launch their own sessions.

For desktop checks use [verify-mediatagger](../../../../.cursor/skills/verify-mediatagger/SKILL.md)
and its feature recipes. Start with `bun run app:control start`, keep the foreground
session alive, require `READY`, then run `doctor`. The controller supplies synthetic media,
temporary XDG directories and a private authenticated display. Record the exact session
identity, title, app-data root and display checks before interaction. Never attach to or
launch an installed production app, production-profile binary or production data.

If development-mode behavior itself needs checking, use `bun run tauri:dev` with fresh
temporary `XDG_DATA_HOME`, `XDG_CONFIG_HOME`, `XDG_CACHE_HOME`, `XDG_RUNTIME_DIR`, synthetic
media and a private display. Inspect [profile isolation](../../../../docs/architecture/system-overview.md)
and current launch configuration first. Do not use the user's normal desktop/profile.

Record commands, working directory, exit results and observations. Inspect screenshots
with the image viewer. Verify persisted effects by reopening/reloading or reading the
temporary database. A successful capture command proves no rendered pixels by itself.
Respect known native rendering and input limitations; unavailable drivers, native dialogs,
video pixels or required tests remain explicit gaps. Do not change app code to make checks pass.

Stop the owned session, confirm processes exited, temporary data was removed and evidence
survived. Controller artifacts may remain in ignored `artifacts/app-control/`; put a concise
sanitized runtime record and evidence copies in `verification/` and link the originals.
