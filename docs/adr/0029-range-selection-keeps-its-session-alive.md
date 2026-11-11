# 0029: A held range selection keeps its query session alive with a one-ID read per minute

- Status: Accepted
- Date: 2026-11-11
- Areas: gallery selection, library query, bulk metadata writes
- Related: [search, tags, and media groups](../subsystems/search-tags-and-media-groups.md#bulk-selection), [library query and gallery](../subsystems/library-query-and-gallery.md#backend-session-and-page-contract), [IPC contract](../architecture/ipc-contract.md#binary-id-ranges), [ADR 0005](0005-binary-id-ranges-and-anchored-refresh.md), [ADR 0008](0008-selection-spec-resolved-in-rust.md)

## Context

ADR 0008 stores a selection of more than `MATERIALIZE_LIMIT` (10,000) positions as ranges of one backend query session, and `apply_selection_mutation` resolves those ranges on that session's ID vector. The backend expires a session after `SESSION_TTL` (five minutes) without a lookup (`src-tauri/src/services/asset_query_service.rs`, `prune`). Only reads restarted that timer, and a held selection performs none. A user who selected everything and then took five minutes to decide on a tag got `stale` from the write, nothing was written, and the refresh it triggered (or the first scroll after the pause) built a new session and cleared the selection. ADR 0008 accepts clearing a range selection when the query result changes, not when the user is idle.

## Options considered

1. **Frontend keepalive (chosen).** While the selection holds ranges, read one ID of their session when they are made and then every minute, through the existing `get_asset_query_id_range`. Any session lookup restarts the TTL, and the read also checks the dependency revision. No IPC, backend, or memory-bound change.
2. **Exempt a session referenced by a selection from the TTL.** The backend does not know which sessions a frontend selection references; telling it needs a new command, or an exemption for each owner's newest session, which changes the cache contract for every caller. Rejected: a contract change for what a periodic read already achieves.
3. **Re-resolve the spec against a fresh session when the write finds its session gone.** The write would rebuild the query and accept the ranges if the dependency revision is still the one the selection was made on. It needs the filters and that revision in the spec (an IPC change) and repeats the most expensive read in the write path. Rejected as more work and more contract for the same outcome.
4. **Lengthen the TTL.** Moves the problem, and keeps up to 64 MiB of ID vectors of abandoned sessions longer. Rejected.

## Decision

- `createSelection` takes an optional `keepAlive(sessionId)`. Exactly while `ranges` is non-empty it reads the ranges' session immediately and then on a `setInterval` of `SESSION_KEEPALIVE_MS` (60 s), with at most one read in flight. The immediate read matters because selecting a large range reads nothing, and the session may already have been idle for most of its TTL; a session that already expired is reported at once. Every `publish` re-evaluates it: clearing, turning selection mode off, an identity reset, materializing into IDs, or a new session (which clears the ranges) stops it; unmount stops it too. A failed read is ignored and the next tick tries again.
- `createAssetQuery.keepSessionAlive(sessionId)` reads the range `0..=0` of that session, only if it is still the current one. A `stale` reply takes the ordinary path: one debounced refresh, whose new session clears the range selection with the "Selection cleared" notice. So a real change of the result (a scan, a reorder, a filter dependency) now clears the selection within about a minute instead of at the next write.
- The backend is unchanged: every lookup by key or session ID, including the ID-range read and `resolve_selection`, restarts the idle expiry, and the dependency revision, not the public revision, decides staleness.

## Consequences

- A range selection survives any idle time while the app runs, and the next bulk write applies to the items the user saw. A range selected on a session that had already expired (more than five minutes without any lookup before the selection) is cleared right away with the notice, because its positions can only be trusted on that session. A write that races a real change is still refused as `stale` and writes nothing; its refresh clears the selection with the notice.
- The cost is one IPC when a range selection starts on a session and then one per minute, each with a 16-byte reply, only while a range selection exists.
- `SESSION_KEEPALIVE_MS` must stay well below `SESSION_TTL`; both constants point to this ADR. Tests: `asset_query_service.rs` (`one_id_keepalive_reads_keep_a_selection_session_past_the_ttl`), `createSelection.test.ts` and `createAssetQuery.test.ts` (fake timers), and `src/app/__tests__/rangeSelectionKeepalive.test.ts` (runtime with a backend double that applies the TTL).
- The keepalive relies on the WebView running timers. The interval leaves a five-fold margin for timer throttling of a hidden or minimized window (not measured on WebKitGTK), and the backend's `Instant` clock does not advance during system suspend. A stall longer than the TTL only brings back the old outcome (stale write, selection cleared with the notice), never a wrong write.
- Other sessions keep the TTL and the four-session LRU. Another window or caller can still evict the selected session (a known limitation of the process-wide cache).
