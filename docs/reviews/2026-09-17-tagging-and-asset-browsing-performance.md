# Tagging and asset browsing performance review

Reviewed revision: `d634c2e`.

Scope: adding tags in lightbox and bulk editors, tag suggestions, asset browsing, and possible performance degradation during long sessions or with 15 or more assigned tags. The working tree was clean and no comparison commit was specified, so this review examined the current implementation. These findings are existing scaling defects, not regressions attributed to a particular diff.

## Findings

### [P2] Avoid scanning the vocabulary for every assigned tag

Location: [src-tauri/src/db/mutations.rs:439](../../src-tauri/src/db/mutations.rs#L439).

Every save calls `canonicalize_tag_rows_in_tx`, which queries `lower(trim(name))` across the entire tag table for each tag already assigned to the asset. The existing indexes on `name` do not index this expression. SQLite's query plan reports `SCAN tags` and a temporary B-tree for ordering. The work grows with assigned tags multiplied by vocabulary size. Bulk tagging repeats it for each selected asset, including assets whose existing names are already canonical.

Synthetic in-memory SQLite measurements using the normalization SQL extracted from the source, with three samples per case:

| Vocabulary entries | Assigned tags | Median normalization time |
| ---: | ---: | ---: |
| 1,000 | 15 | 0.76 ms |
| 10,000 | 15 | 7.48 ms |
| 50,000 | 1 | 2.49 ms |
| 50,000 | 15 | 37.19 ms |
| 50,000 | 30 | 74.55 ms |

These timings cover the repeated normalization lookup only, not the complete save or desktop interaction. Index the normalized lookup or move legacy repair out of routine saves while preserving the existing canonicalization guarantees.

### [P2] Restrict orphan cleanup to tags affected by the edit

Location: [src-tauri/src/db/mutations.rs:760](../../src-tauri/src/db/mutations.rs#L760), calling the query at [line 221](../../src-tauri/src/db/mutations.rs#L221).

Every single-asset tag save performs global orphan cleanup, including additions and unchanged saves. The cleanup builds the distinct tag-ID set from all library assignments and scans the tag table. This work holds the write transaction open and grows with total library assignments even when the edited asset has very few tags.

Synthetic SQLite checks used the cleanup SQL extracted from the source and the relevant table indexes. Each case removed zero orphan rows. Minimum times across three samples were 0.26 ms for 15,000 assignments, 1.80 ms for 150,000 assignments, and 17.63 ms for 1.5 million assignments. These are isolated query timings, not complete application timings.

Limit routine cleanup to tag IDs removed by the edit. Retain global cleanup for maintenance operations that require it.

### [P2] Avoid reloading unchanged vocabulary after every save

Location: [src/hooks/useLibraryKnownTags.ts:21](../../src/hooks/useLibraryKnownTags.ts#L21).

Lightbox and bulk save actions trigger a complete vocabulary refresh. The hook retrieves sequential pages of 200 entries, so a completed refresh of 50,000 tags requires 250 IPC requests even when the user only assigns an existing tag. Overlapping refreshes invalidate the current pagination pass and restart from the beginning. Every completed refresh publishes a new array, which `SuggestionClient` treats as a new vocabulary and sends to the worker for reindexing.

A check using the actual suggestion client and suggestion service issued 15 searches over 50,000 identical tag strings. Keeping the vocabulary array reference produced one vocabulary message; replacing the array with an identical copy before each search produced 15 vocabulary messages. This confirms redundant index rebuilds, but does not measure WebView IPC or worker transfer latency.

Refresh only when vocabulary membership changes, or maintain vocabulary incrementally. Preserve array identity when a refreshed vocabulary is unchanged.

## Verification and limitations

The following focused frontend command passed all 61 tests in eight files:

```sh
bun run test -- src/components/UI/__tests__/TagEditor.test.tsx src/components/search/__tests__/SearchTagator.test.tsx src/components/search/worker/__tests__ src/hooks/__tests__/useLibraryKnownTags.test.ts src/hooks/__tests__/useLibraryAssets.test.tsx src/hooks/services/__tests__/thumbnailStore.test.ts src/components/app/hooks/__tests__/useAssetTagState.test.ts
```

Additional checks ran extracted application SQL against synthetic in-memory SQLite data and exercised the actual suggestion client with a worker substitute. Benchmark scripts ran inline and were not retained. The measurements above record their output; they are not a benchmark suite.

The reviewed code bounds gallery pages, backend query sessions, detail caches, and inactive per-asset tag state. This review did not establish a long-session memory leak or a special failure threshold at 15 tags. The confirmed costs depend on library size and tag count.

Desktop interaction timing, a long-session soak test, and complete Rust save-path timing remain unverified. No application source was changed. This report was saved after the initial read-only review at the user's request.
