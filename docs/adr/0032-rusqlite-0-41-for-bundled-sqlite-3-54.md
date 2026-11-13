# 0032: rusqlite 0.41 with bundled SQLite 3.54 for RUSTSEC-2026-0459

- Status: Accepted
- Date: 2026-11-13
- Areas: dependencies, database, backup and restore, data safety
- Related: [database](../subsystems/database.md), [data safety and portability](../subsystems/data-safety-and-portability.md), [ADR 0010](0010-zip-9-deflate-only.md) (the same audit gate), [ADR 0002](0002-backend-write-priority-and-scan-revisions.md) (busy handling on the bundled build)

## Context

`bun run quality` failed in `audit:rust` on RUSTSEC-2026-0459 (CVE-2026-119375, CVSS 6.8): SQLite before 3.54.0 can return uninitialized page-cache memory when it opens a database together with a `-wal` file whose page size differs from the database's. The application built `rusqlite` 0.32.1 with the `bundled` feature, so it linked `libsqlite3-sys` 0.30.1 and SQLite 3.46.0. The advisory's fix is `libsqlite3-sys` >= 0.39.0 (SQLite 3.54.0).

The application is exposed through restore: bundle validation accepts `media.db-wal` and `media.db-shm` entries and writes them next to the staging database before opening it (`src-tauri/src/services/backup_service.rs`, `extract_database_entries`). A crafted bundle could therefore make validation or the restored library read stale process memory.

`libsqlite3-sys` declares `links = "sqlite3"`, so only one version can be in the graph, and the only crate that depends on it is `rusqlite`. The first `rusqlite` that requires `libsqlite3-sys` 0.39 is 0.41.0 (MSRV 1.89; the toolchain is 1.98). `rusqlite` 0.42.0 (`libsqlite3-sys` 0.40) was two days old and is not needed for the fix.

## Options considered

1. **Ignore the advisory in `audit:rust`.** No code change, but keeps a known memory disclosure on the restore path and hides later findings for the same crate. Rejected.
2. **Pin or patch `libsqlite3-sys` 0.39 under `rusqlite` 0.32.** `rusqlite` 0.32 requires `libsqlite3-sys` ^0.30 and the `links` key allows one copy, so this needs a `[patch]` fork of `rusqlite` or of the sys crate's version. Rejected: an unmaintained fork of the database binding.
3. **Link the system SQLite (drop `bundled`).** The SQLite version would then depend on the build host and the Flatpak/AppImage runtimes, and older distributions ship SQLite below 3.54. Rejected.
4. **`rusqlite` 0.42.** Also fixed, but released two days before this change and one more major step of the sys crate. Rejected for now; nothing here prevents a later bump.
5. **`rusqlite` 0.41 (chosen).** The smallest version that carries the fix, locked at 0.41.1 with `libsqlite3-sys` 0.39.1 (SQLite 3.54.1).

## Decision

- `src-tauri/Cargo.toml`: `rusqlite = { version = "0.41", features = ["backup", "bundled", "fallible_uint", "hooks"] }`. `cargo update -p rusqlite` changed only `rusqlite` 0.32.1 → 0.41.1, `libsqlite3-sys` 0.30.1 → 0.39.1 and `hashlink` 0.9.1 → 0.12.2; it removed `ahash` 0.8.12 and `hashbrown` 0.14.5 (`hashlink` now uses the existing `hashbrown` 0.17.1, which gains its `foldhash` dependency), and added `sqlite-wasm-rs` and `rsqlite-vfs`, which are dependencies of the `wasm32-unknown-unknown` target only and are not built for Linux.
- Breaking changes between 0.32 and 0.41 that reached this code, and how they were handled:
  - The `usize`/`u64` `ToSql`/`FromSql` impls became opt-in in 0.38. The `fallible_uint` feature restores exactly the earlier checked conversions (an `i64` overflow or a negative value is an error), used by `COUNT(*)` reads into `usize` and a `usize` bind.
  - `progress_handler`, `authorizer`, `commit_hook` and `busy_handler` now take `&mut self` and return `Result` (the closure is kept as SQLite client data). The query session's cancellation handler propagates the error with `?` (`asset_query_service.rs`). When a pooled connection is dropped, a connection whose progress handler cannot be cleared is closed instead of returned to the pool (`db_pool.rs`, `Drop for PooledConnection`); clearing is not expected to fail. Test helpers that install hooks take `&mut Connection` and unwrap the result.
- Breaking changes checked and not reached: `Connection::execute` and `prepare` reject SQL with a second statement (0.35; every multi-statement string already goes through `execute_batch`, and the full backend suite passes); PRAGMA names are checked as identifiers only (0.41, without `extra_check`); `Backup` takes names through the `Name` trait with the same `main` default; `Statement::column_names` returns an iterator; the statement cache stays in the default `cache` feature with capacity 16; `Transaction::new_unchecked`, `params!`, `OptionalExtension` and `execute`'s `usize` result are unchanged.
- The bundled build no longer defines `SQLITE_ENABLE_MEMORY_MANAGEMENT` (0.41). The application never calls `sqlite3_release_memory` or sets a soft heap limit, so nothing depends on it. `HAVE_USLEEP` (ADR 0002) and `SQLITE_DEFAULT_OPTIMIZE_LIMIT` 2000 inside `PRAGMA optimize` are unchanged.

## Consequences

- `bun run quality` passes, including `audit:rust`. The informational "unmaintained"/"unsound"/"yanked" warnings in transitive dependencies of Tauri and its GTK stack are unchanged.
- No schema, query-semantics or IPC change. Databases are opened by a newer SQLite; the file format is unchanged and older builds can still open them.
- A restore that opens a bundled `-wal` with a mismatched page size reads no uninitialized memory. No regression test builds such a crafted pair: the fix is in SQLite, and the audit gate checks the version.
- Measured with the ignored `grid_baseline` diagnostics (release, disk-backed `TMPDIR`, both builds on the same host): query rebuilds, first and cached section reads, page and ID-range reads change by -12% to +1% (unfiltered 1M rebuild 31.4 → 31.7 ms, first 1M sections read 27.6 → 27.1 ms, in the ranges of ADR 0007). The ranged group writes that change rows stay in the ranges of ADR 0026 (1M assign all 3.1 s, clear all 2.6 to 2.8 s, a tenth 0.71 to 0.92 s of lock-out, against 0.73 to 0.79 s before). One step is consistently slower: a ranged group write that changes no row (`reassign_all_same`, which stages every ID one `INSERT` at a time and compares) takes 72 to 73 ms instead of 63 ms at 100,000 rows and 768 ms instead of 666 to 675 ms at 1,000,000 rows (+14 to +16%, three runs each). `Statement::execute` is unchanged between the two `rusqlite` versions, so the difference is in SQLite. This cost was accepted for the security fix: the step writes nothing, its lock-out stays well below the 5 s busy budget, and no other measured path regressed.
- The measured environment in [tag-save-benchmark.md](../development/tag-save-benchmark.md) still names rusqlite 0.32.1, because it records the build those numbers came from.
- Hook closures must be installed on an owned, mutable connection. New code that installs one on a pooled connection needs `let mut conn` and must handle the `Result`.
