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

Option 2 now, and option 3 as the target (stage 08 of the plan).

- **Rule 1.** The identifiers `tauri`, `tauri_plugin_*`, `AppHandle`, `gtk`, `gdk`, `glib`, `webkit2gtk`, `libmpv2`, `libmpv2_sys`, `async_channel` and `libloading` appear only in the adapter: `lib.rs`, `main.rs`, `commands/**`, `adapter/**`, `video/**` and `app/profile.rs`. They may also appear in a transitional allowlist that only shrinks.
- **Rule 2.** `db.rs` and `db/*` do not reference `services`, `app`, `commands`, `usecases` or the adapter modules.
- **Rule 3.** No core file references `commands`, `adapter`, `video` or `video_surface`. A transitional allowlist holds `video_surface.rs` until stage 04.
- **Rule 4** (from stage 06). `commands/**` contains no `db::`, `with_*lock` or `Connection`; handlers call use cases.
- **D1.** The whole libmpv/GTK video stack lives in the adapter. The core keeps only `video_source_service`, and there is no video event port.
- **D2.** The core becomes a separate crate as the last structural stage.
- **D3.** The core crate is named `media_tagger_core`, following the internal naming policy.
- **D4.** Integration tests stay in `src-tauri/tests` and import from the core crate.
- The core keeps `rusqlite` (no repository traits) and `serde`: `models.rs` is both domain and wire format. The core does not depend on `http`; `thumb://` replies are plain data that the adapter turns into `tauri::http::Response`.
- The wire contract does not change in any stage: command names, argument keys, response fields, event names and error texts.

`src-tauri/tests/architecture_boundaries.rs` enforces rules 1–3 and keeps rule 4 as an ignored test until stage 06. The same target freezes the wire contract:
- the `generate_handler!` list and its documentation in the IPC contract;
- every command's wire arguments and return type;
- the JSON shape of the response, input and video-channel DTOs;
- the emitted event names.

## Consequences

- A new leak of the GUI runtime into the core fails `bun run test:backend`.
- An allowlist entry that is no longer needed also fails the test, so the allowlists can only shrink. Only the plan orchestrator edits them.
- An intended IPC change updates the frozen snapshot in `architecture_boundaries.rs` in the same change. This is now a step of the IPC contract-change checklist.
- Error texts are not in the snapshot. The plan moves code verbatim and reviews the diff instead.
- No schema, app-data, profile or wire change. Stage 08 adds a local package entry to `Cargo.lock` and touches Dockerfiles and packaging.
- The scanner matches tokens after removing comments and literals. Code generated by macros is not seen. Stage 08 replaces rule 1 with the compiler and `cargo tree -p media_tagger_core`, and the scanner keeps rules 2–4.
- This ADR becomes Accepted in stage 09, when the core crate exists and the documentation describes the final layout.
