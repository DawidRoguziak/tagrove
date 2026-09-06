# Backend refactor verification

Verified on Linux on 2026-09-06 in the existing checkout. Tests used temporary databases and media. Desktop checks used the isolated E2E profile on private Xvfb displays.

## Implemented behavior

The application-owned database runtime admits each operation once and keeps that admission alive through connections and child workers. Maintenance drains admitted work before closing idle connections and taking workflow locks. Blocking command work runs outside the async executor; thumbnail cancellation touches atomics directly.

Source mutations use indexed validation and dedicated FULL-synchronous connections. Rename rollback tracks the move separately from directory-sync success. Precise fingerprint changes update thumbnail invalidation and record versions together. Schema version 3 versions thumbnail failures and discards unversioned legacy failure markers while preserving asset metadata.

Bulk thumbnails use 512-row keyset pages and persist each page before advancing. Streamed ready events follow accepted database writes; stale versions remain retryable. CSV records, restore paths, collision keys, and restored thumbnail lookups use SQLite staging. Completed CSV and ZIP exports pass importer limits before publication; exported files use ZIP64 to support the 16 GiB per-entry limit. Query sessions retain at most four ID vectors within a 64 MiB capacity budget and interrupt superseded SQL through progress callbacks.

Native playback submits load initialization asynchronously through its event client. Rendering wakes through a channel; the worker and control updater wait while no session is active. Database code is split into connections, schema, queries, mutations, scan membership, and thumbnails.

## Automated checks

| Check | Result | Local evidence |
| --- | --- | --- |
| `cargo test --manifest-path src-tauri/Cargo.toml --locked --offline` | 175 passed; one opt-in measurement test excluded | `/tmp/backend-tests.log` |
| Opt-in metadata measurements, command below | 1 passed | `/tmp/backend-scale.log` |
| Cargo Clippy, all targets/features, warnings denied | Passed | `/tmp/backend-clippy.log` |
| Cargo formatting check | Passed | `/tmp/backend-rust-format.log` |
| `bun run test` | 429 passed in 72 files | `/tmp/backend-all-frontend-tests.log` |
| `bun run typecheck`, `bun run lint`, `bun run format:check` | Passed | `/tmp/backend-typecheck.log`, `/tmp/backend-lint.log`, `/tmp/backend-format.log` |
| `app.workflows.e2e.js` on private Xvfb | All 13 passed | `/tmp/backend-e2e.log` |
| Restore desktop workflow after final staging and ZIP64 changes | Passed | `/tmp/backend-restore-e2e.log` |
| `bun run test:all` on private Xvfb | Passed: quality, 429 frontend, 175 Rust, 18 desktop tests; 1 Rust and 10 desktop opt-in tests skipped; 18 allowed Rust audit warnings | `/tmp/mediatagger-test-all.log` |

`bun run format:check` checks scripts and Vite/Vitest configuration under the repository's current script definition; it does not format the changed React/TypeScript files.

Rust regressions cover admission/maintenance interleavings, pool draining, SQLite interruption during sorting, callback removal on reuse, cache epochs and capacity, cancellation while maintenance holds locks, stale publication, frontend request-number restarts, same-second changes, versioned failures, and schema-2 migration. Safety cases include transaction rollback, directory-sync failure after successful moves, simulated restore recovery before and after commit, and small injected CSV/archive limits, and actual ZIP64 headers with an importer round trip. A real scan indexes 514 temporary images across overlapping roots and crosses the 512-row batch boundary. It also checks missing-root partial completion and root-removal membership. Traversal faults at every directory position are not injected.

The desktop suite exercises thumbnails, tags, root removal, library clearing, lightbox deletion, CSV round trips, bundle restore, video replacement, paused seeking, and fullscreen. These are unbundled debug builds, not release-package verification.

## Metadata measurements

Reproduce with:

```sh
cargo test --manifest-path src-tauri/Cargo.toml --locked --offline --test backend_runtime metadata_scale_measurements -- --ignored --nocapture --test-threads=1
```

The fixture creates metadata only, with no large media collection. Results are one local debug-build run, not latency targets or a million-asset benchmark.

| Rows | Ordered-ID query | Rust ID-vector capacity | Additional peak SQLite allocation during query | Full path read VM steps | Indexed filename read VM steps | Candidate pages at size 7 |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 1,000 | 0.994 ms | 8,192 bytes | 87,584 bytes | 4,006 | 14 | 143 |
| 10,000 | 9.066 ms | 131,072 bytes | 375,584 bytes | 40,006 | 14 | 1,429 |

`EXPLAIN QUERY PLAN` selects `idx_assets_file_name_key`. The VM-step comparison measures the old full path read against its indexed replacement, not complete rename latency. Seven-row candidate pages return every fixture row without exceeding the requested bound. Production bulk candidate and result batches remain capped at 512; scheduler worker and in-flight limits remain unchanged.

SQLite allocation uses its process-global memory high-water counter reset after fixture setup, so this measurement runs alone. Rust vector capacity is measured separately. The 64 MiB query-cache budget covers retained ID capacities only. It excludes SQLite working memory, pooled connection caches, and vectors held by concurrent readers. This run does not measure total application RSS at one million assets.

## Native display evidence and remaining limits

The persistent desktop controller loaded a video, observed playback time advance, closed it, and reloaded the gallery. Artifacts are under `artifacts/app-control/acbd588fa2d68f1f5db18347dd4fa708/`. `screen-0001.png` shows native controls but a black picture area. This host's Xvfb limitation is also recorded in the [desktop control skill](../../.cursor/skills/verify-mediatagger/SKILL.md). Native decoded pixels and hardware GTK/OpenGL behavior still require a real-display check. Null-output playback and control-state assertions do not prove rendering.

Archive tests use injected small limits; no near-128-GiB archive, 4-GiB CSV, or two-million-entry fixture was created. CSV parsing still allocates an individual record before field validation, and ZIP directory validation retains entry metadata. Disk staging bounds the application record batches, not every parser allocation. Recovery tests construct interrupted on-disk states and inject selected sync failures; they do not simulate actual power loss or every filesystem failure. FULL durability still depends on the filesystem and device honoring synchronization.

The local decision trail is `/tmp/backend-refactor-decisions.tsv`. It is append-only; later verification rows supersede earlier pending statuses. Logs and screenshots are working artifacts and are not committed test fixtures.
