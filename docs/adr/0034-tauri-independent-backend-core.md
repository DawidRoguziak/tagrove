# 0034: The backend core does not depend on Tauri; Tauri is an adapter

- Status: Proposed
- Date: 2026-11-14
- Areas: backend architecture, module boundaries, IPC, build
- Related: [system overview](../architecture/system-overview.md#backend-module-boundaries), [IPC contract](../architecture/ipc-contract.md), plan [hexagonal-backend](../plans/hexagonal-backend/README.md), [ADR 0006](0006-thumb-uri-scheme.md), [ADR 0012](0012-non-blocking-native-video-render.md)

## Context

The Rust crate `media_tagger` holds the backend core (SQLite, queries, mutations, scanning, thumbnails, backup, CSV, workflow locks, bootstrap) and the Tauri desktop shell in one module tree. On commit `397181f`, 14 files name Tauri, GTK, WebKit or libmpv:

- The adapter by nature: `lib.rs`, the six `commands/*` files and `video_surface.rs`.
- Leaks into lower layers:
  - `app/state.rs` uses `tauri::async_runtime` channels for the startup scan.
  - `services/progress.rs` implements `ProgressSink` for `AppHandle`.
  - `services/thumb_protocol.rs` keeps `UriSchemeResponder` and `AppHandle` inside its publisher.
  - `services/video_player_service.rs` and `services/video_events.rs` use `tauri::ipc::Channel`, `libmpv2` and `libmpv2_sys`.
  - `services/thumb_service.rs` names `tauri::http` in one test.

Everything else is already framework-free: `db.rs` and `db/*`, `indexer.rs`, `thumbs.rs`, `models.rs`, `error.rs`, `utils/*`, `app/{locks,instance_lock,profile}.rs` and most services. The core is synchronous (rusqlite, std threads, `mpsc`), and `spawn_blocking` appears only at the edge.

Nothing stops a new leak today. The use-case logic sits in private `*_service` functions in `commands/*.rs`, and `video_surface.rs` imports `commands::video` types. The plan moves this code in several steps. That needs a measurable boundary, and it needs proof that the IPC wire contract does not change on the way.

## Options considered

1. **Status quo.** No cost, but the boundary keeps eroding and core code cannot be built or tested without the GUI stack.
2. **A logical boundary inside the crate, enforced by a source-scanning test.** Cheap, and incremental per stage. It is not compiler-enforced, and a token scanner can be fooled by macros.
3. **A separate core crate `media_tagger_core` in a Cargo workspace.** The compiler enforces the boundary. It needs a `Cargo.lock` entry and changes to Dockerfiles and packaging. It is cheap only after option 2 has emptied the leaks.
4. **Full hexagonal architecture**: repository traits over SQLite, ports for the filesystem, clock, environment and ffmpeg, async in the core, and separate domain/application/infrastructure crates. Rejected:
   - each port would have exactly one implementation;
   - the invariants (revision bump in the same transaction, sort-key triggers, `DatabaseRuntime` permits) are bound to SQL transactions;
   - the tests already run on in-memory SQLite and temporary directories.

## Decision

Option 2 first (stages 01–07 of the plan), then option 3 (stage 08). Since stage 08 the core is the crate `media_tagger_core` in `src-tauri/crates/media_tagger_core`, in one Cargo workspace with the adapter package `media_tagger`. The rules below were scanner rules until stage 08; rules 1 and 3 are now enforced by the compiler.

- **Rule 1.** Tauri, its plugins, GTK, GDK, GLib, WebKit, libmpv, `async-channel` and `libloading` are used only by the adapter package. The core manifest names none of them, so the core cannot compile against them; `core_manifest_has_no_gui_dependencies` rejects such a dependency in any table of the core manifest (until stage 08: a token scan with an allowlist that shrank to empty in stage 04).
- **Rule 2.** The core's `db.rs` and `db/*` do not reference `services`, `app` or `usecases` (source scan).
- **Rule 3.** The core does not reference the adapter (`commands`, `adapter`, `video`, the `media_tagger` crate). Since stage 08 this does not compile. Inside the adapter, `video/` does not import `commands/` (source scan).
- **Rule 4** (from stage 06). The adapter's `commands/**` contains no `db::`, `with_*lock`, `locks` or `Connection`; handlers call use cases (source scan).
- **Orphan rule.** A core port is implemented in the adapter on an adapter type: `TauriProgress(&AppHandle)` implements `ProgressSink`, because `impl ProgressSink for AppHandle` is E0117 across crates. The wire event stays `process-progress`.
- **Test fixtures.** The core's `test_support` (ADR 0028 fixtures) compiles under `cfg(any(test, feature = "test-support"))`. Only the adapter's `[dev-dependencies]` enable `test-support`, for the player tests; a release build never contains it.
- **D1.** The whole libmpv/GTK video stack lives in the adapter. The core keeps only `video_source_service`, and there is no video event port.
- **D2.** The core becomes a separate crate as the last structural stage.
- **D3.** The core crate is named `media_tagger_core`, following the internal naming policy.
- **D4.** Integration tests stay in `src-tauri/tests` and import from the core crate.
- The core keeps `rusqlite` (no repository traits) and `serde`: `models.rs` is both domain and wire format. The core does not depend on `http`; `thumb://` replies are plain data that the adapter turns into `tauri::http::Response`.
- The wire contract does not change in any stage: command names, argument keys, response fields, event names and error texts.

`src-tauri/tests/architecture_boundaries.rs` checks the core manifest (rule 1) and scans both packages for rules 2–4. The same target freezes the wire contract:
- the `generate_handler!` list and its documentation in the IPC contract;
- every command's wire arguments and return type;
- the JSON shape of the response, input and video-channel DTOs;
- the emitted event names.

## Consequences

- A new leak of the GUI runtime into the core fails to compile, or, as a manifest change, fails `--locked` resolution and `core_manifest_has_no_gui_dependencies`. `cargo tree -p media_tagger_core --locked -e normal` lists no GUI crate.
- The core builds and tests without the GUI stack: `cargo build -p media_tagger_core --locked`, `cargo test -p media_tagger_core --locked`. Both packages are workspace `default-members`, so `bun run test:backend`, `rust:clippy` and `rust:fmt` cover both.
- There are no allowlists left; the transitional ones and their tests were removed in stage 08.
- An intended IPC change updates the frozen snapshot in `architecture_boundaries.rs` in the same change. This is now a step of the IPC contract-change checklist.
- Error texts are not in the snapshot. The plan moves code verbatim and reviews the diff instead.
- No schema, app-data, profile or wire change. Stage 08 adds a local package entry `media_tagger_core` to `Cargo.lock` (the external dependencies and their versions are unchanged) and touches the Dockerfiles (`COPY` of the core manifest and a stub `lib.rs` for the fetch layer), the notices collector, the release-input check and the privacy-check examples.
- The scanner matches tokens after removing comments and literals. Code generated by macros is not seen. Rules 2 and 4 depend on it; rules 1 and 3 no longer do.
- This ADR becomes Accepted in stage 09, when the core crate exists and the documentation describes the final layout.
