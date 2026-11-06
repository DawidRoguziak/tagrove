# 0002: Prioritize user metadata writes and throttle scan revision bumps

- Status: Accepted
- Date: 2026-11-07
- Areas: database pool, scanning, library revisions, thumbnails, workflow locks
- Related: [database persistence](../subsystems/database.md), [scanning and indexing](../subsystems/scanning-and-indexing.md), [thumbnails](../subsystems/thumbnails.md), [system overview](../architecture/system-overview.md#in-process-locking-policy), [plan stage 02](../plans/grid-performance/02-backend-contention.md), [ADR 0001](0001-per-asset-metadata-write-queue.md)

## Context

Since ADR 0001, the frontend queues every favorite and tag write, so a slow backend shows directly as a pending heart or tag. The grid-performance baseline (`e2e/specs/grid-baseline.e2e.js`, plan stage 07) measured writes during a 20,000-file scan:

- Debug E2E: tag writes up to 3,032 ms and favorites up to 1,532 ms. Every slow write fell inside a scan phase.
- Release (`src-tauri/tests/grid_baseline.rs`): favorite writes p95 129 ms; tag writes p95 529 ms and max 639 ms.
- Each scan bumped the library revision 45–53 times. A gallery reading a page every 100 ms rebuilt its ID session 4–5 times per one-second scan.

The pool had spare connections (wait ≤ 0.6 ms). The cause is how SQLite serializes writers:

- A scan commits 512-row batches almost back to back.
- Our bundled SQLite (`HAVE_USLEEP`) sleeps with a backoff that reaches 100 ms per retry. A waiting user write therefore keeps missing the short gaps between batches.
- A test with background transactions of 30 ms each (`user_write_is_not_starved_by_back_to_back_background_transactions`) showed this without the priority: the default handler never got the lock, and the write failed after four 5-second busy timeouts.

Three smaller costs added to the contention:

- Thumbnails wrote twice per result: a compare-and-set UPDATE, then a failure DELETE. A visible tile whose thumbnail already existed still took the write lock twice.
- Render and ensure commands held the shared `thumb_lock` for their whole run, including scheduler waits of up to ~90 s for video. Linux `RwLock` lets a waiting writer block new readers, so one delete or rename stopped all gallery thumbnails behind the longest render.
- Each scan held two pooled connections, out of four.

## Options considered

1. **Only enlarge the pool or shrink scan batches.** Neither changes the ratio between lock hold time and the gaps, so the polling writer still misses them.
2. **One process-wide mutex around every SQLite write transaction.** Fair, but every write path, including low-level helpers and autocommit statements, would need to take it. A missed site would silently bypass it, and it would add a new lock to the documented order. Rejected.
3. **Millisecond busy polling on every connection.** Simple, but it is still probabilistic when a scan holds the lock most of the time, and background writers would compete as hard as user writes. Rejected as the only measure.
4. **Separate "scan progress" revision that the frontend refreshes on a timer.** Needed only if something durable depended on per-batch bumps. Nothing does: query sessions and duplicate snapshots live in process memory, startup rebuilds the dependency markers, and duplicate resolution takes the scan lock and so sees the final bump. Rejected as unnecessary IPC and frontend work (plan stage 04).
5. **User-write priority plus scan revision throttling (chosen).**

## Decision

**Pool** (`src-tauri/src/services/db_pool.rs`):

- At most 8 open connections. Ordinary `OperationPermit::connection()` checkouts are limited to 6 at a time.
- `user_write_connection()` may use all 8. Set and bulk favorite, tag and media-group commands use it (`commands/assets.rs`).
- While a user-write checkout is alive, it holds a ticket that background writers can see. Its connection polls a busy write lock every millisecond within the usual 5 s budget (`db::poll_busy_lock_quickly`). Dropping the checkout restores the 5 s busy timeout before the connection returns to the idle pool.
- `yield_to_user_writes()` makes a background writer wait, before it begins a transaction, while any ticket exists. It waits at most 250 ms per transaction.

**Scan** (`services/scan_service.rs`, `commands/scan.rs`):

- A scan holds exactly one pooled connection. The registration and root-list connections are returned before the scan starts.
- Batch, touch and prune transactions are `BEGIN IMMEDIATE`, and each yields to user writes first.
- Changed-file batches bump the library revision inside their transaction for the first batch, then at most once per second (`ScanRevisionThrottle`).
- A prune that removes assets still bumps in its own transaction.
- The final bump after all roots is unconditional, and its failure fails the scan.
- If the scan ends with an error or panic after committing an unbumped batch, `ScanWriter`'s `Drop` bumps once, best effort.

**Thumbnails** (`services/thumb_service.rs`, `db/thumbnails.rs`, `app/locks.rs`):

- Render and ensure commands enter through `with_thumb_operation`, which takes no lock. Services take `thumb_read_section` around snapshot reads, enqueueing, publication commits and file removals, and never while waiting for the scheduler.
- `commit_thumbnail_results` writes each result's version-checked path and its failure marker in one IMMEDIATE transaction.
- Stream results are published in groups made of whatever is already finished, with no timer.
- Bulk pages publish in transactions of up to 256 rows, each yielding to user writes.
- A stored path read together with its source version in one snapshot is ready without a write. A write happens only when a stale failure marker must be cleared.
- Bulk renders capture the clear epoch per candidate page.

## Consequences

- During a scan, a favorite or tag write waits for at most the scan transaction already in progress, plus millisecond polling.
- A scan bumps the revision about once per second, so the frontend rebuilds sessions about once per second.
- Readers can briefly see a slightly stale session. For at most a second, new rows can be missing and changed rows can keep their old position. The final bump, and the error-path bump, always restore consistency.
- A delete, rename, root removal or clear waits only for in-progress short thumbnail sections, not for renders. New thumbnail work waits only for that exclusive writer.
  - A clear can now interleave with a running coordinator. The clear epoch drops results from before the clear and removes their files, as it already did for scheduler work left behind by a cancelled bulk.
- Failure markers now commit in the same transaction as their path. A database error there fails the command instead of being silently ignored.
- New invariants:
  - user metadata writes use `user_write_connection()`;
  - background bulk writers call `yield_to_user_writes()` before each transaction;
  - render/ensure code holds the thumbnail read guard only in short sections.

  They are tested in `db_pool.rs` (reserve, timeout restore, bounded yield), `tests/backend_runtime.rs` (exhausted ordinary checkouts, back-to-back background transactions), `scan_service.rs` (throttle, error-path bump, one connection, revision count of a real scan), `commands/scan.rs` (one connection per scan command), `db/thumbnails.rs` (one commit per result group) and `thumb_service.rs` (a waiting exclusive writer does not stall new work; an existing thumbnail needs no write lock; failure cleared with its path).
- No schema, IPC payload, app-data or dependency change. The `ensure_thumbnails` `ready` rule now also accepts an existing path confirmed with its version in one snapshot (`docs/architecture/ipc-contract.md`).
