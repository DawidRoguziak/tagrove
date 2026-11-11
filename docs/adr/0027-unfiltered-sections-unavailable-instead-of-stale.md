# 0027: A row-count mismatch of lazy unfiltered sections returns no sections and keeps the session

- Status: Accepted
- Date: 2026-11-11
- Areas: library query, IPC, gallery, scanning
- Related: [library query and gallery](../subsystems/library-query-and-gallery.md#timeline-sections-and-scrubber), [IPC contract](../architecture/ipc-contract.md#asset-queries-and-reads), [ADR 0002](0002-backend-write-priority-and-scan-revisions.md), [ADR 0007](0007-session-timeline-sections.md)

## Context

ADR 0007 reads the sections of an unfiltered session lazily, on the first `get_asset_query_sections` call, from the live sort-key index in the read transaction that checks the session's dependency revision. Its consequences required the section row count to equal the session total and made a mismatch `stale`; the code removed the session and called a mismatch "a broken invariant, not a race".

ADR 0002 made that reachable. A scan commits its 512-row batches between throttled revision bumps (at most one per second, plus a final and an error-path bump; `src-tauri/src/services/scan_service.rs`, `flush_scan_batch`, `ScanWriter::finish`, `ScanWriter::drop`). A batch that lands between the start's snapshot and the sections' snapshot passes the revision check but changes the count. The gallery's session was removed, its pending pages turned `stale`, and the frontend rebuilt the ID vector (once per second at most, `scheduleStaleRefresh`) while the timeline stayed hidden or delayed during the scan (audit record `S-49d8a0ad`).

The session itself is not wrong in that window: its IDs and pages are the snapshot the dependency revision promises, with the bounded staleness ADR 0002 accepts. Only the lazily read sections no longer describe it.

## Options considered

1. **Keep `stale` and remove the session (status quo).** Correct sections, but it throws away a valid session and its ID vector because of an optional UI element, and repeats the rebuild while batches keep landing. Rejected.
2. **Build unfiltered sections from the session's own vector.** Exact, but it needs each ID's bucket time: a per-ID lookup over up to 1,000,000 rows, or the bucket columns in the ID build, which ADR 0007 measured as doubling the unfiltered rebuild. Rejected.
3. **Compare a write counter that every commit moves.** No such counter exists; `PRAGMA data_version` is per connection and the pool hands out different connections. Adding one would be a new revision rule for a cosmetic feature. Rejected.
4. **Return `ready` with no sections, keep the session, store nothing (chosen).** The timeline is optional UI; the frontend already hides it for an empty list. The scan's next bump makes the session `stale` anyway, and the session built after it reads its sections again.

## Decision

- In `AssetQueryManager::sections` (`src-tauri/src/services/asset_query_service.rs`), when the lazily read unfiltered sections' counts do not sum to the session total, the result is `ready` with `session_id`, `total` and an empty `sections` list. The session is not removed and the sections are not stored, so a later read of the same session reads the index again.
- A dependency-revision mismatch still makes every read of the session, including sections, `stale` and removes the session.
- In the IPC contract, `ready` with an empty list and `total > 0` means "sections unavailable for this snapshot". The response shape does not change. `createAssetQuery` handles it as before: the timeline stays hidden and no refresh is scheduled.

## Consequences

- During a scan, an unfiltered session that races a batch commit keeps serving pages, positions, ID ranges and cached starts; only its timeline stays hidden until the next bump replaces the session. The scan always bumps when it ends or fails. The settings workflow refreshes the library after a successful scan, so the final session gets its sections; after a failed manual scan the old session turns `stale` on its next read, and the refresh that follows builds a session with sections.
- A batch that only updates existing rows' `modified_at` keeps the count, so those sections can be slightly misaligned with the vector until the same bump ends the session. This is the bounded staleness of ADR 0002 applied to the labels.
- A real invariant break (a write that changes results without a bump outside a scan) now shows as a hidden timeline rather than a refresh. Such writes are already a bug under the library-revision rule.
- Tests: `asset_query_service.rs` (`unbumped_scan_commit_keeps_the_session_without_sections_until_the_next_bump`), `backend_integration.rs` (`sections_during_a_scan`, with a real scan for the final bump), `createAssetQuery.test.ts` and `api.test.ts` for the frontend handling and the wire shape.
- This amends the count rule in ADR 0007's consequences; the rest of ADR 0007 stands.
