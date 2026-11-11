# 0024: A failed thumbnail keeps retrying with a capped backoff while its tile is mounted, and a returning source is requested again

- Status: Accepted
- Date: 2026-11-11
- Areas: thumbnails (`thumb://` demand), gallery tiles
- Related: [thumbnails](../subsystems/thumbnails.md#frontend-tiles), [library query and gallery](../subsystems/library-query-and-gallery.md), [ADR 0006](0006-thumb-uri-scheme.md), [ADR 0021](0021-visible-tiles-drop-a-pending-prefetch.md)

## Context

[ADR 0006](0006-thumb-uri-scheme.md) gave `ThumbnailImage` a fixed budget: two retries after 500 ms and 2 s, then a transparent placeholder that stayed. The scheduler refuses new targets while `MAX_PENDING_JOBS` (2048) jobs are queued or running, and `thumb://` answers that with `503` and `Retry-After: 1`. WebKitGTK cannot cancel requests, so after a long steady scroll over unrendered videos (two browsing video jobs at a time) the queue can stay full for minutes with jobs for tiles already scrolled past. The tiles visible at the stop got `503` for longer than 2.5 s and stayed blank (`S-e83e88a0`, audit 20261108-1145).

The failure was also remembered per `src` for the component's lifetime. A recycled row slot that showed another asset and then the same one again built the same URL and got the placeholder from memory, without a request (`S-0d1cb214`).

An `<img>` error event carries no HTTP status, so the frontend cannot tell a transient `503` from a final `404` (missing source, failure already recorded in this process).

## Options considered

1. **Signal "busy" in a way the `<img>` can see**, for example a `200` with a marker picture for a full queue. It would let the tile retry only on `503`, but it changes the protocol contract and serves a picture that is not a thumbnail under a cacheable scheme. Rejected for this fix.
2. **Bound the stale backlog in the scheduler** so a visible request is never refused. It needs a cancellation signal that WebKitGTK does not give, or dropping queued jobs whose waiters still expect a response. Rejected.
3. **Keep retrying in the frontend while the tile is mounted** (chosen). The quick retries and the placeholder stay as before; after them the same element requests the source again with a doubling delay from 4 s, capped at 30 s, until it loads, the source changes or the component unmounts. Retry and failure state are keyed by each change of `src`, so a returning source starts again with a fresh request.

## Decision

- `ThumbnailImage` (`src/ui/controls/ThumbnailImage.tsx`) retries after 500 ms and 2 s (`THUMBNAIL_RETRY_DELAYS_MS`), then shows the transparent placeholder and calls `onFailed` once for that showing of the source. While the component stays mounted with the same `src`, it keeps retrying through `retrySrc` after 4 s, 8 s, 16 s and then every 30 s (`thumbnailRetryDelay`). Between the slower retries the placeholder is shown, so the tile spinner (CSS only, after 300 ms) shows only while such a request is slow. A load clears the placeholder and reports `onReady`.
- One timer per component at most. It is cleared on unmount and when `src` changes; a timer that fires for an earlier source does nothing.
- The retry counter passed to `retrySrc` cycles through 1 to 999, so `r` keeps at most three digits.
- Retry and failure state are keyed by an object created each time the `src` string changes, not by the string itself. A source that comes back to the same element is requested again; a re-read of an unchanged URL (a refreshed or patched summary) keeps the pending retry and its backoff.
- Without `retrySrc` (a cached-only request) a failure is final for that showing, as before.

## Consequences

- A tile refused by a full queue recovers within 30 s of the queue accepting it again, without scrolling away and back.
- A tile whose thumbnail fails for good (missing source, broken video) sends one request about every 30 s while it is mounted. The backend answers it from the per-process failure memory without rendering; a missing source costs a `Path::exists` and a failure-marker write per request. With a screen of such tiles (about 150) that is about 5 requests per second.
- A failed prefetch tile keeps `p=1` on its slower retries, because `GalleryTile` treats a final failure as settled ([ADR 0021](0021-visible-tiles-drop-a-pending-prefetch.md)); once admitted, its render waits behind Interactive jobs.
- Tested in `src/ui/controls/__tests__/ThumbnailImage.test.tsx` with fake timers: a failure longer than the quick retries that then loads, the delay cap and counter cycle, a returning source on a recycled element (with and without `retrySrc`), an unchanged URL read again during a pending retry, and unmount cancelling the pending retry.
- The other `retrySrc` users (`GroupOrderModal`, `DuplicateAssetCard`) behave the same while their rows are mounted; their lists are virtual and their modals unmount on close.
- No IPC, schema or profile change.
