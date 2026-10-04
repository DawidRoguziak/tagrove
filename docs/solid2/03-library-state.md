# Stage 3: Shared library state

Previous: [platform and shell](02-platform-and-shell.md).
Next: [gallery and search](04-gallery-and-search.md).
Read the [architecture and completion rules](README.md) and update [progress](PROGRESS.md).

## Outcome

Provide application-owned library models that coordinate reads and writes independently
of mounted views, with the same cache, retry, and data-safety behavior as React.

## Implementation

- Implement the query session model with 128-item pages, sparse global-index access,
  per-offset request deduplication, failed-page tracking, and the exact 12-page bound.
- Keep old results visible during refresh until a current first page is ready. Preserve
  superseded-start handling, stale-session recovery, explicit retry, and generation guards.
- Retain existing 256-entry details cache bounds and metadata-generation validation.
  Restore/clear and query-session changes invalidate the correct identities and caches.
- Implement shared canonical tag state, per-asset write exclusion, pins, tombstones,
  identity epochs, and the 512-entry inactive-record retention bound.
- Keep maintenance admission and write draining synchronous and explicit. Rejection or
  disposal must not strand a mutation token, waiter, or global maintenance barrier.
- Centralize successful tag, favorite, media-group, and deletion publication so bulk
  and lightbox see consistent state. Await the backend before publishing canonical data.
- Preserve query-impact decisions and vocabulary refresh behavior, including unchanged
  saves, dirty retry state, overlapping refreshes, and stable published vocabulary.
- Reuse the framework-independent thumbnail store where suitable. Add Solid per-asset
  subscriptions and preserve viewport priority, additive consumers, request generations,
  cancellation, stream completion, retries, retention, and atomic tile updates.
- Keep operational maps, locks, and counters as synchronous coordination state. Expose
  reactive read state and explicit commands instead of exported mutable setters.

## Tests and acceptance

- Port library-assets/browser, known-tags, tag coordinator, thumbnail queue/store, and
  mutation tests. Reuse pure service tests directly.
- Exercise eviction and reload beyond 12 pages, concurrent range reads, failed offsets,
  explicit retry, stale responses, refresh during navigation, and disposal during reads.
- Add same-turn duplicate mutation and maintenance-admission checks under Solid batching.
- Verify canonical writes reject old details, preserve unrelated queries, and invalidate
  identities correctly after restore, clear, and deletion.
- Verify thumbnail completion updates only affected subscribers and releases registrations.
- Test failure paths with controlled deferred responses; do not replace races with sleeps.

## Review and local commit

Review ownership, cache bounds, mutation ordering, synchronous exclusion, stale-result
guards, and disposal. Apply the complete stage gate from the README.

Intended commit: `feat: implement Solid library state and mutation coordination`.
