---
paths:
  - "src-tauri/src/lib.rs"
  - "src-tauri/src/main.rs"
  - "src-tauri/src/app/**"
  - "src-tauri/src/adapter/startup_scan.rs"
  - "src-tauri/src/error.rs"
  - "src-tauri/tauri*.conf.json"
  - "src-tauri/capabilities/**"
  - "src-tauri/Cargo.toml"
---

# Backend bootstrap, state, locks, profiles

Read `docs/architecture/system-overview.md` (startup order, locking policy, profiles, security surface).

- Profiles: release `com.example.mediatagger`, dev `.dev`, E2E `.e2e`. A debug build refuses any identifier other than dev/E2E (`app/profile.rs`). Never change an identifier: the user's existing library would look empty.
- `instance.lock` (per profile) is acquired before `media.db` opens.
- Startup order in `lib.rs::run` matters (the `thumb://` scheme is registered on the builder and answers 503 until `AppState` exists): restore recovery → best-effort sweep of abandoned backup temporaries (ADR 0016) → `thumbs/` → DB + `init_schema` → pending file-op recovery → popular-tag and startup-scan snapshots → ffmpeg/LC_NUMERIC/video player → thumbnail scheduler (startup fails if a worker fails to spawn) → `AppState`.
- Lock order is fixed: maintenance gate → `scan_lock` → exclusive `thumb_lock`. Ordinary work takes an operation permit first. Use the helpers in `app/locks.rs` (`with_scan_and_thumb_lock`, `with_database_maintenance`; thumbnail work enters with `with_thumb_operation`, a permit only, and takes `thumb_read_section` per short section, never while waiting for the scheduler). Never enter maintenance while holding a lower lock, because that deadlocks.
- Take workflow locks inside `spawn_blocking`, never across an `.await`. Locks recover from poisoning with `into_inner`.
- The frontend settings runner is a UI convenience, not a backend safety boundary. Direct IPC callers can run commands concurrently.
- Security surface: CSP is `null`, the asset-protocol scope is `**`, and the opener is limited to the GitHub repo URL. Review these together when you change any of them.
