# Frontend refactor verification

Verified on 2026-09-06 with the existing React/Tauri stack and isolated Linux E2E profile. No worktree, migration, million-item dataset, or expanded performance fixture was used.

## Correctness evidence

Final checks passed: 428 frontend tests across 72 files, 159 Rust unit/workflow/integration tests, lint, TypeScript checking, production build, Rust formatting, and locale validation for all 11 languages.

The frontend suite covers validated toolbar submission, pipe-containing query keys, tag-count invalidation, persistent page failures with explicit retry, whole-range cancellation on refresh/reset/unmount, metadata arriving after writes, and dirty group drafts. Selection regressions cover eviction, filter changes, omitted summaries, partial acknowledgements, changed selection during writes, and four concurrent hydration batches. Worker tests cover request/vocabulary ordering, coalescing, cleanup, and failure recovery. Cache tests cross the 512-record bound and reject evicted generations.

The real desktop workflow confirmed validation without changing the applied results, tag persistence after reopening, group persistence in the temporary database, and worker suggestions. Its evidence is under `artifacts/app-control/38336707f6d30dd5eb79c7269af2f7b7/`. The controller stopped successfully and removed its temporary profile.

The gallery E2E scenario additionally verifies ordered summary IPC, the 256-ID input limit, and all three selected items receiving tags, grouping, and favorites after page eviction and a filter excluding them. Both desktop cases passed. The initial run exposed a mount-effect replay bug: cancelling the first read without restarting it could leave the gallery empty. The query effect now restarts during replay, with a StrictMode regression.

## Existing desktop performance scenario

The fixture remains 2,048 PNGs plus its existing GIF/video media. The cold and warm passes follow the same route through all 16 pages and revisit evicted pages. The scenario asserts fewer than 200 mounted gallery tiles and checks static GIF thumbnails and animated lightbox playback.

| Measurement | Cold | Warm |
| --- | ---: | ---: |
| Frame interval p95 | 18 ms | 19 ms |
| Scroll-to-thumbnail-ready p95 | 494 ms | 507 ms |
| Frame samples | 541 | 226 |

These measurements include WebDriver overhead on a private Xvfb display. They are observations of this run, not a comparison with a measured pre-change desktop baseline. Raw timings and screenshots are in `artifacts/gallery-performance/`.

## Scale constraints

| Owner | Retention/work bound |
| --- | --- |
| Gallery query | 12 pages, normally 128 summaries each; virtual indices refer to the session total without materializing frontend rows for every match. |
| Lightbox and bulk detail caches | 256 entries each. Lightbox snapshots also validate mutation generations. |
| Canonical tag state and generation clocks | 512 inactive IDs, plus pinned editors and active mutations. Eviction cannot reuse a generation. |
| Selected summaries | Only explicitly selected IDs; at most four batches of 256 IDs in flight. Deselection releases metadata. |
| Bulk ordering DOM | Visible virtual rows plus three rows of overscan. Thumbnail demand follows that range. |
| Thumbnail updates | Per-item subscriptions; production shell state contains no mirrored path/rendering maps. Paths outside cached pages and active preview subscriptions are released. |
| Suggestions | One lazy worker per shell; one running search and one pending search per active editor. Vocabulary indexing and searching stay outside React rendering. |

The backend's existing query session still stores ordered matching IDs. This change preserves that architecture. Million-asset suitability was assessed through ownership, complexity, retention limits, and virtualization, not a million-item test.
