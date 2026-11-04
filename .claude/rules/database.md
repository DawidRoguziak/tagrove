---
paths:
  - "src-tauri/src/db.rs"
  - "src-tauri/src/db/**"
  - "src-tauri/src/services/db_pool.rs"
  - "src-tauri/src/services/asset_query_service.rs"
---

# SQLite

Read `docs/subsystems/database.md`.

- There is no migration runner. `init_schema` upgrades in place and must be safe to re-run. Gate one-time data fixes on a new `performance_schema_version` value, and never reuse an old value with a new meaning.
- Any write that changes what a query returns bumps the library revision (`db/query_revisions.rs`) in the same transaction. Thumbnail writes never bump it.
- Ordinary connections: WAL, `synchronous=NORMAL`, foreign keys, 5 s busy timeout. File-mutation journals and startup recovery use separate `synchronous=FULL` connections that never enter the pool.
- Tags are trimmed and lowercased. Whitespace, comma, semicolon, and control characters are invalid. The backend is authoritative (`utils/tags.rs`).
- Blocking SQL runs on `spawn_blocking`, and services take runtime permits explicitly.
- Tests use `tempfile` or in-memory SQLite. Add `backend_integration` coverage for multi-module transactions.
