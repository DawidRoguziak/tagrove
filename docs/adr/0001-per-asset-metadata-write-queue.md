# 0001: Queue metadata writes per asset instead of rejecting busy writers

- Status: Accepted
- Date: 2026-11-06
- Areas: frontend metadata coordination, lightbox, bulk editing, settings maintenance
- Related: [search, tags and media groups](../subsystems/search-tags-and-media-groups.md#metadata-coordination), [lightbox](../subsystems/lightbox.md), [frontend architecture](../frontend/architecture-and-ui-conventions.md), [plan stage 01](../plans/grid-performance/01-mutation-queue.md)

## Context

Every tag, favorite, media-group and delete write takes an exclusive per-asset token in `src/library/metadata/createAssetTagState.ts` before its IPC call. Until now a second writer on the same asset was rejected (`"Metadata is busy"`) without contacting the backend. The lightbox serialized only its own tag saves, and bulk had separate per-kind locks.

The grid-performance baseline (`e2e/specs/grid-baseline.e2e.js`, plan stage 07) reproduced the user-visible bug "liking does not work". Adding a tag and clicking the heart right away failed 9–15 of 20 favorite clicks under scan load, with no `set_asset_favorite` call, while the backend returned no write errors. The exclusive token is still needed: it keeps detail reads that overlap a write from publishing stale metadata, and it lets settings maintenance (CSV import, restore, clear) drain writes before it runs.

## Options considered

1. **Reject and offer Retry (previous behavior).** Simple, but a correct click fails whenever another field of the same asset is being saved. Every editor needs its own retry UX, and the failure is common during scans because writes take longer.
2. **Serialize all metadata writes globally.** Simple ordering, but unrelated assets wait for each other, so one slow bulk write stalls every single-asset edit.
3. **FIFO queue per asset in the coordinator (chosen).** Writes on the same asset run in admission order; writes on different assets stay concurrent.

## Decision

`createAssetTagState.enqueueMutation(ids)` admits a write behind earlier writes on any of its IDs and returns a turn with `begin()` (starts the exclusive tokens) and `release()`. `beginMutation` is no longer public; every write goes through the queue.

- Admission links every target ID to the new turn synchronously, so dependencies follow admission order. Overlapping multi-asset writes cannot deadlock, and IDs need no sorting.
- An idle write starts in the same tick. A busy one starts when it reaches the head of every target queue. A failed write still releases its slot.
- A write is rejected only when maintenance is active at admission, when its asset was deleted or the identity epoch changed while it waited, or after disposal. An identity reset starts empty queues for the new epoch.
- The maintenance barrier and `waitForIdle` count admitted writes, queued or running, so maintenance drains writes that were admitted before it.
- `createMutations.write` reads its cached-details base when the write starts, not when it was queued. `saveTags` accepts a function that computes the complete replacement from canonical tags at start. Bulk single-asset edits and lightbox replacements use it, so a queued edit merges onto the earlier result instead of undoing it.
- Feature models keep pending counters only for presentation. Controls stay disabled during their own write as a double-click guard.

## Consequences

- A heart click, tag edit or group save during another pending write on the same asset is saved after it, in click order. "Metadata is busy" no longer exists.
- A slow backend now shows a longer pending state instead of an error. Plan stage 02 addresses backend contention during scans.
- A queued replacement must not capture a complete tag list at click time; editors that replace the whole list pass a function or rebase their intent at start (`createLightbox.drain`).
- Enforced and tested in `src/library/metadata/__tests__/createAssetTagState.test.ts` (FIFO, failure release, overlapping bulks, barrier, reset, deletion, disposal), `mutations.test.ts`, `createLightbox.test.ts`, `createBulk.test.ts`, `src/app/__tests__/runtimeIntegration.test.ts` and `src/features/settings/__tests__/operationSafety.test.tsx`.
- No schema, IPC or app-data impact. The backend is unchanged.
