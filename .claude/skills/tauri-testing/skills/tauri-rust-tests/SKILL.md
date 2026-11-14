---
name: tauri-rust-tests
description: Write and fix Rust tests for Tagrove's backend workspace in src-tauri (core crate media_tagger_core, Tauri adapter media_tagger) - #[cfg(test)] unit tests beside usecases/services/db code, serde contract tests for IPC inputs (camelCase) and responses (snake_case), tempfile and in-memory SQLite fixtures, db::test_support, the core's test-support feature, and the integration targets in src-tauri/tests (backend_integration, backend_e2e, backend_runtime, thumbnail_*, tag_*, grid_baseline) with tests/support. Use whenever a task touches tests under src-tauri/.
---

# Rust tests in Tagrove (`src-tauri` workspace: `media_tagger_core` and `media_tagger`)

Read `tauri-test-strategy` first if the layer is not decided. Owning docs:
`docs/development/testing.md` → "Rust unit and integration tests", plus the subsystem page for
the code under test (database, scanning, thumbnails, data safety; see CLAUDE.md's index).

## 1. Where tests go

`src-tauri/` is a Cargo workspace with two packages (ADR 0034): the core crate
`media_tagger_core` (`src-tauri/crates/media_tagger_core/src/`: `usecases/`, `services/`, `db/`,
`app/`, `indexer.rs`, `thumbs.rs`, ..., no Tauri/GTK/libmpv) and the Tauri adapter
`media_tagger` (`src-tauri/src/`: `lib.rs`, `commands/`, `adapter/`, `video/`, `app/profile.rs`).

- **Unit tests**: a `#[cfg(test)] mod tests` at the bottom of the module, beside the code. Almost
  everything testable lives in the core (`src-tauri/crates/media_tagger_core/src/usecases/*.rs`,
  `services/`, `db/`, `app/bootstrap.rs`, `indexer.rs`, `thumbs.rs`, ...). This is the default for
  helpers, parsing/normalization, SQL semantics, validation, error texts, scheduler behavior and
  use-case/service branches; none of them needs a running Tauri app. Command handlers in
  `commands/*.rs` are thin and must not contain `db::`, `Connection`, `admit` or lock helpers, not
  even in tests (rule 4 of `architecture_boundaries`), so their tests go into the matching
  `usecases/*.rs`. Adapter unit tests cover `adapter/*`, `commands/video.rs` and `video/*`.
- **Integration targets**: `src-tauri/tests/*.rs` (they belong to the adapter package), against
  the public `media_tagger_core` API (`media_tagger` only for commands and video types).
  Current targets: `architecture_boundaries` (core/adapter import rules and the frozen
  IPC wire snapshot, ADR 0034), `backend_e2e` (Rust-only, not desktop), `backend_integration`,
  `backend_runtime`, `grid_baseline`, `tag_save_benchmark`, `tag_vocabulary`,
  `thumbnail_bulk`, `thumbnail_clear`. Add to an existing target before creating a new one.
  Multi-module transactions belong in `backend_integration`.
- Shared integration helpers live in `src-tauri/tests/support/` (`mod support;`), e.g.
  `support::assets::session_page`. Unit tests use the crate-private equivalents in
  `db::test_support` (`src-tauri/crates/media_tagger_core/src/db.rs`) and
  `services::thumb_scheduler::test_support`. `#[cfg(test)]` items are not visible from `tests/`
  or from the other package.
- The ADR 0028 fixtures in the core's `test_support.rs` compile under
  `cfg(any(test, feature = "test-support"))`; only the adapter's `[dev-dependencies]` enable
  `test-support` (for the player tests in `video/player.rs`). Adding a dependency or feature
  changes `Cargo.lock` and needs the user's approval.

## 2. Fixtures and isolation

- `tempfile::tempdir()`, held for the whole test. Never write into the repo, a fixed path, or
  real app data (`~/.local/share/com.example.mediatagger*`).
- Database-only: `Connection::open_in_memory()`. File-backed behavior: `tempdir()/media.db` +
  `db::open_connection` + `db::init_schema`.
- Scan roots and media live under the temp dir. Small byte strings are fine when only path
  identity/metadata matter; generate a real image (as the scan-service tests do) when decoding
  is the behavior. No personal media, no private paths in fixtures (public repo).
- Backup/ZIP fixtures and every restore destination stay in the temp dir. For destructive code
  assert both the DB result and which files survived or were removed.
- Scheduler tests inject a processor and synthetic paths; parser tests use literal tool output.
  Don't depend on a local `ffmpeg` unless the executable boundary is the subject.
- Cargo runs tests in parallel: no shared DB, scan root, output filename or mutable global. If a
  test must change process-wide state, serialize it explicitly or inject the state.
- A write that changes query results must bump the library revision in the same transaction;
  when you test such a write, assert the revision too (`db/query_revisions.rs`).

## 3. IPC contract tests (Rust side)

Tagrove does **not** use `tauri::test` / `MockRuntime` (the `tauri` `test` feature is not
enabled; enabling it is a dependency decision that needs an ADR). Commands are thin wrappers
returning `Result<T, String>` (an `AppError` mapped to a string) around a use case in
`usecases/*`, so the Rust half of an IPC contract is proved with plain unit tests in the use-case
(or, for video, the command) module:

- **Responses**: build the response struct and `serde_json::to_value` it; assert the
  snake_case field names and tagged discriminants (`status: "ready" | "stale" | ...`) exactly as
  `src/platform/tauri/contracts/` declares them. Example:
  `selection_summary_batch_limit_and_group_response_contract` in
  `src-tauri/crates/media_tagger_core/src/usecases/assets.rs`.
- **Inputs**: deserialize the JSON the frontend sends. Top-level argument keys are camelCase
  (`assetId` ↔ `asset_id`); nested inputs follow their own `rename_all`. Example:
  `parses_camel_case_track_control_fields` in `src-tauri/src/commands/video.rs`. Add a negative
  case when the shape is strict (wrong casing or missing tag must fail).
- **Validation/limits**: test the validator the command calls (`validate_summary_batch`,
  `validate_file_name`, ...) for boundary values and the exact error text as a literal (example:
  `validation_errors_keep_their_wire_text` in `usecases/assets.rs`); error texts are wire contract.
- The frontend half goes in `src/platform/tauri/__tests__/api.test.ts` (`tauri-solid-tests`);
  the real bridge is proved only by desktop E2E (`tauri-e2e-tests`).
- An IPC change also touches the `generate_handler!` list in `src-tauri/src/lib.rs`, the
  wrapper, its `contracts/` type and `docs/architecture/ipc-contract.md` in the same change,
  and updates the frozen snapshot in `src-tauri/tests/architecture_boundaries.rs`
  (command list, wire signatures, DTO JSON). A refactor that keeps the contract leaves it unchanged.

## 4. Async, locks and runtime

- Locks are taken inside `spawn_blocking` and never held across `.await`; lock order is
  maintenance gate → `scan_lock` → exclusive `thumb_lock` via `app/locks.rs`. Tests for
  concurrency should exercise those helpers, not hand-rolled locks.
- Wait for completion through channels/joins or injected processors, never `thread::sleep`.
- The libmpv player regression (`cargo test --manifest-path src-tauri/Cargo.toml --locked video_ --lib`)
  needs ffmpeg and libmpv with null outputs; it does not cover GTK/OpenGL.

## 5. Running

```bash
cargo test --manifest-path src-tauri/Cargo.toml --locked <name>                      # focused, both packages
cargo test --manifest-path src-tauri/Cargo.toml --locked -p media_tagger_core <name> # core only, no GUI stack
cargo test --manifest-path src-tauri/Cargo.toml --locked -p media_tagger --lib <name> # adapter unit tests
cargo test --manifest-path src-tauri/Cargo.toml --locked --test backend_integration <name>
bun run test:backend                                                                 # whole layer, both packages
bun run rust:fmt && bun run rust:clippy                                              # -D warnings, all targets
# ignored diagnostics/benchmarks: add -- --ignored --nocapture --test-threads=1
```

List targets: `cargo metadata --manifest-path src-tauri/Cargo.toml --locked --offline --no-deps --format-version 1`.
Do not add dev-dependencies (rstest, proptest, insta, tokio test-util, nextest) without asking;
`tempfile` and `serde_json` are already available.

## 6. Anti-patterns

- Asserting on `Debug` output when a typed field or error text check exists.
- `unwrap()` without a message on fallible setup; use `expect("what failed")` like the existing tests.
- Tests depending on execution order, on a shared fixed DB, or on files left by another test.
- Marking a failing test `#[ignore]` to get green. `#[ignore]` is only for opt-in diagnostics and benchmarks.
