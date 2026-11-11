# Architecture Decision Records

This folder holds Tagrove's Architecture Decision Records (ADRs). An ADR captures one significant decision: the context it was made in, the options that were considered, what was chosen, and the consequences. The owning pages in `docs/` describe how the system works now; ADRs explain why it works that way.

## When to write an ADR

Write an ADR in the same change that introduces the decision when it:

- changes an architecture invariant from `CLAUDE.md` or `docs/architecture/` (module boundaries, import direction, state ownership, lock order, startup order);
- changes the SQLite schema, query semantics, ordering, or library-revision rules;
- adds, changes, or removes an IPC command, event, or channel contract in a way that is not purely additive;
- adds or replaces a significant dependency, runtime, or toolchain (for example the video backend, the virtualizer, the search engine, or the package manager);
- affects data safety: deletion, restore, backup format, or the file-mutation journal;
- affects profiles, app-data isolation, packaging, or the security surface;
- deliberately rejects a reasonable alternative that someone is likely to propose again.

Do not write an ADR for bug fixes, refactors that keep behavior and boundaries, UI copy, or test-only changes. When in doubt, write a short one.

## How to write one

1. Copy `template.md` to `NNNN-short-kebab-title.md`, using the next free four-digit number. Numbers are never reused.
2. Fill in every section. Keep it short: usually one screen. Link to code (`path:line`), the owning `docs/` page, and the plan in `docs/plans/` if one exists.
3. Set the status to `Proposed` while the change is under review and to `Accepted` when it lands.
4. Update the owning `docs/` page in the same change, so the current behavior is documented there and not only in the ADR.
5. Add a row to the index below.

The same public-repository rules apply as everywhere else: no secrets, private paths, personal media, or private data in examples.

## Lifecycle

An accepted ADR is not rewritten. To change a decision, write a new ADR and set the old one's status to `Superseded by NNNN`, linking both ways. The only edits allowed to an accepted ADR are its status line, links to related ADRs, and fixes to typos or broken links.

Statuses: `Proposed`, `Accepted`, `Rejected`, `Deprecated`, `Superseded by NNNN`.

Plans in `docs/plans/` describe staged work. When a plan stage makes a decision that meets the criteria above, record it as an ADR and link the ADR from the plan.

## Index

| ADR | Title | Status | Date |
| --- | --- | --- | --- |
| [0001](0001-per-asset-metadata-write-queue.md) | Queue metadata writes per asset instead of rejecting busy writers | Accepted | 2026-11-06 |
| [0002](0002-backend-write-priority-and-scan-revisions.md) | Prioritize user metadata writes and throttle scan revision bumps | Accepted | 2026-11-07 |
| [0003](0003-custom-gallery-virtualizer.md) | Own row-pool virtualizer with a windowed native scroll, without TanStack virtual-core | Accepted | 2026-11-07 |
| [0004](0004-trigger-maintained-gallery-sort-keys.md) | Trigger-maintained gallery sort keys with an exact filtered order | Accepted | 2026-11-07 |
| [0005](0005-binary-id-ranges-and-anchored-refresh.md) | Binary ID ranges and anchored query refresh | Accepted | 2026-11-07 |
| [0006](0006-thumb-uri-scheme.md) | Gallery thumbnails through a `thumb://` URI scheme | Accepted | 2026-11-07 |
| [0007](0007-session-timeline-sections.md) | Timeline sections of query sessions | Accepted | 2026-11-07 |
| [0008](0008-selection-spec-resolved-in-rust.md) | Bulk selection as a spec resolved by the backend | Accepted | 2026-11-07 |
| [0009](0009-remove-list-assets-command.md) | Remove the legacy `list_assets` command | Accepted | 2026-11-07 |
| [0010](0010-zip-9-deflate-only.md) | `zip` 9 with the Deflate backend only | Accepted | 2026-11-07 |
| [0011](0011-audit-qa-data-safety.md) | Audit QA runs on a reflink copy inside a sandbox | Accepted | 2026-11-08 |
| [0012](0012-non-blocking-native-video-render.md) | Native video rendering never waits on the GTK main thread | Accepted | 2026-11-10 |
| [0013](0013-committed-delete-cleanup-never-blocks-startup.md) | Committed-delete cleanup never blocks startup and keeps its journal row until the file is verifiably gone | Accepted | 2026-11-10 |
| [0014](0014-single-asset-file-mutations-guarded-by-row-cas.md) | Single-asset file mutations are guarded by the row compare-and-set, not by the library revision | Accepted | 2026-11-10 |
| [0015](0015-restore-tolerates-unresolved-thumbnails-and-nullable-file-name.md) | Restore drops unresolved thumbnail references and accepts the nullable `file_name` column of an earlier migration | Accepted | 2026-11-10 |
| [0016](0016-startup-sweep-of-abandoned-backup-temporaries.md) | Startup removes abandoned backup temporaries by exact name, and a failed pre-swap restore removes its staging | Accepted | 2026-11-11 |
| [0017](0017-export-target-validated-before-creating-folders.md) | Export targets are validated before any folder is created, and the indexed-asset check reads assets only for an existing target | Accepted | 2026-11-11 |
| [0018](0018-render-all-keeps-thumbnails-of-unreachable-sources.md) | Render all keeps the cached thumbnail of an unreachable source and writes only rows that change | Accepted | 2026-11-11 |
| [0019](0019-clear-epoch-keeps-current-version-targets.md) | A result dropped by the clear epoch keeps a target whose source version is still current | Accepted | 2026-11-11 |
| [0020](0020-exif-orientation-and-legacy-thumbnail-adoption.md) | Image thumbnails apply EXIF orientation, and cache version 3 adopts a legacy thumbnail only when the picture is unchanged | Accepted | 2026-11-11 |
| [0021](0021-visible-tiles-drop-a-pending-prefetch.md) | A tile that scrolls into view before its prefetched thumbnail arrives drops `p=1`, and High stays FIFO | Accepted | 2026-11-11 |
| [0022](0022-lightbox-delete-fails-fast-during-a-scan.md) | Lightbox delete fails fast while a scan holds the scan lock instead of waiting for it | Accepted | 2026-11-11 |
| [0023](0023-app-control-unresponsive-state-and-out-of-queue-stop.md) | app:control reports an unresponsive app instead of failing, allows only native Escape there, and runs stop outside the command queue | Accepted | 2026-11-11 |
| [0024](0024-thumbnail-retries-continue-while-the-tile-is-mounted.md) | A failed thumbnail keeps retrying with a capped backoff while its tile is mounted, and a returning source is requested again | Accepted | 2026-11-11 |
