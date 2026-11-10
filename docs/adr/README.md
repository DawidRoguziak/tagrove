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
