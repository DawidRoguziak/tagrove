---
paths:
  - "src/platform/tauri/**"
  - "src-tauri/src/commands/**"
  - "src-tauri/src/models.rs"
  - "src-tauri/src/lib.rs"
  - "src-tauri/src/services/progress.rs"
---

# IPC contract

Read `docs/architecture/ipc-contract.md` before changing a command, payload, channel, or event.

- Only `src/platform/tauri/*` calls `invoke`, listens to events, or builds Tauri channels. Components and models consume the typed wrappers. Wire types live in `src/platform/tauri/contracts/*.ts`.
- One change updates every layer together: the Rust `#[tauri::command]`, the `models.rs` type, the `generate_handler!` list in `src-tauri/src/lib.rs`, the TS wrapper, the `contracts/` type, its tests, and the IPC doc. The two sides are kept in sync by hand, with no codegen.
- Argument keys are camelCase in JS; response fields are snake_case (serde defaults). Errors cross as plain strings. Never branch on error wording.
- An unregistered `#[tauri::command]` cannot be called from JS. `rename_asset_file` and the single-asset/page thumbnail helpers exist but are deliberately not registered.
- `process-progress` is one shared, best-effort, uncorrelated event. Consumers filter by phase, and phase names are part of the contract. Progress never proves an operation finished; only the command result does.
- Native video is opened by asset ID only. The backend resolves and authorizes the path, so the frontend never sends a path.
- Tests: a Rust test for the command plus `src/platform/tauri/__tests__/api.test.ts` for the payload shape. A mocked `invoke` does not prove Rust accepts the payload, so a real bridge needs desktop E2E.
