---
paths:
  - "src-tauri/src/lib.rs"
  - "src-tauri/src/main.rs"
  - "src-tauri/src/app/**"
  - "src-tauri/src/usecases/**"
  - "src-tauri/src/adapter/startup_scan.rs"
  - "src-tauri/src/adapter/thumb_scheme.rs"
  - "src-tauri/src/video/mod.rs"
  - "src-tauri/src/error.rs"
  - "src-tauri/tauri*.conf.json"
  - "src-tauri/capabilities/**"
  - "src-tauri/Cargo.toml"
---

# Backend bootstrap, state, locks, profiles

Read `docs/architecture/system-overview.md` (startup order, locking policy, profiles, security surface).

- Profiles: release `com.example.mediatagger`, dev `.dev`, E2E `.e2e`. A debug build refuses any identifier other than dev/E2E (`app/profile.rs`). Never change an identifier: the user's existing library would look empty.
- `instance.lock` (per profile) is acquired before `media.db` opens.
- Startup order matters (the `thumb://` scheme is registered on the builder, `adapter/thumb_scheme.rs`, and answers 503 until `AppState` is managed). `lib.rs::run` only composes; the framework-free phases are in `app/bootstrap.rs`, which never calls `manage` and imports nothing from `adapter`, `commands`, `video` or `usecases`. Order: identifier guard → `app_data_dir` → `InstanceLock` → `open_storage` (restore recovery → best-effort sweep of abandoned backup temporaries (ADR 0016) → `thumbs/` → DB + `init_schema` + FULL sync → pending file-op recovery → popular-tag and scan-root snapshots) → `manage` of `StartupPopularTags` and `StartupScanState` → `resource_dir` → `resolve_ffmpeg_path` → `LC_NUMERIC` → `video::player::VideoPlayerService::new` → `video::surface::setup` → `manage(player)` → `start_workers` (thumbnail scheduler, startup fails if a worker fails to spawn; builds `AppState`) → `manage(AppState)` as the last step. The startup connection lives until `setup` returns. Tests: `app::bootstrap::tests`.
- Lock order is fixed: maintenance gate → `scan_lock` → exclusive `thumb_lock`. Ordinary work takes an operation permit first. Use the helpers in `app/locks.rs` (`with_scan_and_thumb_lock`, `with_database_maintenance`; thumbnail work enters with `with_thumb_operation`, a permit only, and takes `thumb_read_section` per short section, never while waiting for the scheduler). Never enter maintenance while holding a lower lock, because that deadlocks.
- Take workflow locks inside `spawn_blocking`, never across an `.await`. Locks recover from poisoning with `into_inner`.
- Command handlers (`commands/*`) are thin: arguments → `spawn_blocking` → use case → `map_err(String)` / `ipc::Response`. Validation, the `app/locks.rs` helpers and `db::` calls live in `usecases/*` (one file per command file, `<command>_service`); rule 4 of `src-tauri/tests/architecture_boundaries.rs` rejects `db::`, `Connection`, `with_*lock` and any `locks` path in `commands/`. `commands/video.rs` stays in the adapter (ADR 0034, D1).
- The frontend settings runner is a UI convenience, not a backend safety boundary. Direct IPC callers can run commands concurrently.
- Security surface: CSP is `null`, the asset-protocol scope is `**`, and the opener is limited to the GitHub repo URL. Review these together when you change any of them.
