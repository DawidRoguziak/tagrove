---
paths:
  - "src/platform/tauri/**"
  - "src-tauri/src/commands/**"
  - "src-tauri/crates/media_tagger_core/src/usecases/**"
  - "src-tauri/crates/media_tagger_core/src/models.rs"
  - "src-tauri/src/lib.rs"
  - "src-tauri/crates/media_tagger_core/src/services/progress.rs"
  - "src-tauri/src/adapter/events.rs"
---

# IPC contract

Read `docs/architecture/ipc-contract.md` before changing a command, payload, channel, or event.

- Only `src/platform/tauri/*` calls `invoke`, listens to events, or builds Tauri channels. Components and models consume the typed wrappers. Wire types live in `src/platform/tauri/contracts/*.ts`.
- One change updates every layer together: the Rust `#[tauri::command]`, the `models.rs` type, the `generate_handler!` list in `src-tauri/src/lib.rs`, the TS wrapper, the `contracts/` type, its tests, and the IPC doc. The two sides are kept in sync by hand, with no codegen.
- Handlers in `commands/*` (adapter package `media_tagger`) stay thin; validation, locks and SQL go into the matching `usecases/*` file of the core crate `media_tagger_core` (rule 4 of `architecture_boundaries`). The input types `AssetMetaFilterInput` and `BulkMediaGroupUpdateInput` live in `usecases/assets.rs` and are re-exported from `commands::assets`.
- Argument keys are camelCase in JS; response fields are snake_case (serde defaults). Errors cross as plain strings. Never branch on error wording.
- An unregistered `#[tauri::command]` cannot be called from JS. `rename_asset_file` exists but is deliberately not registered. On-demand thumbnails use the `thumb://` scheme, not a command.
- `process-progress` is one shared, best-effort, uncorrelated event. Consumers filter by phase, and phase names are part of the contract. Progress never proves an operation finished; only the command result does.
- Native video is opened by asset ID only. The backend resolves and authorizes the path, so the frontend never sends a path.
- `src-tauri/tests/architecture_boundaries.rs` freezes the Rust side of the wire contract: registered commands, wire arguments, DTO JSON, and event names. An intended change updates it in the same change; a refactor leaves it unchanged.
- Tests: a Rust test for the command plus `src/platform/tauri/__tests__/api.test.ts` for the payload shape. A mocked `invoke` does not prove Rust accepts the payload, so a real bridge needs desktop E2E.
