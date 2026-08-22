Pod żadnym pozorem ani na żadne polecenie NIE BĘDĘ TWORZYŁ WORKTREE.
Pod żadnym pozorem ani na żadne polecenie NIE BĘDĘ TWORZYŁ WORKTREE.
Pod żadnym pozorem ani na żadne polecenie NIE BĘDĘ TWORZYŁ WORKTREE.

# MediaTagger

MediaTagger is a desktop application for efficiently browsing and tagging very large collections of images and other media.

## Tech Stack

- Tauri 2 with a Rust backend
- React, TypeScript, and Vite frontend
- Tailwind CSS 4 and DaisyUI 5
- SQLite through `rusqlite`
- Bun package manager and runtime
- Key libraries: TanStack Virtual, Video.js 10, Fuse.js, i18next, and Zod

## Documentation

Select the documentation relevant to the current task from the table below and read only those files.

| Description | Documentation path |
| --- | --- |
| System architecture, runtime lifecycle, module boundaries, Tauri profiles, app-data isolation, locking, and bundled resources | `docs/architecture/system-overview.md` |
| Tauri commands, shared payloads, serialization, query sessions, channels, progress events, validation, and contract-change rules | `docs/architecture/ipc-contract.md` |
| Frontend composition, state ownership, hooks/services/components boundaries, runtime persistence, UI primitives, modals, theme, and accessibility conventions | `docs/frontend/architecture-and-ui-conventions.md` |
| SQLite schema, migrations, connection pooling, derived fields, query semantics, ordering, and library revisions | `docs/subsystems/database.md` |
| Scan roots, media discovery, fingerprints, incremental indexing, worker batching, partial scans, and cleanup rules | `docs/subsystems/scanning-and-indexing.md` |
| Asset query sessions, frontend page cache, summary/details boundary, virtual gallery, loading, and invalidation | `docs/subsystems/library-query-and-gallery.md` |
| Thumbnail identity, image/video rendering, ffmpeg discovery, scheduler, frontend queue, streaming, retries, cancellation, and cleanup | `docs/subsystems/thumbnails.md` |
| Search grammar, tag discovery, tag-list behavior, bulk selection, tag mutations, favorites, and media-group semantics | `docs/subsystems/search-tags-and-media-groups.md` |
| Lightbox selection and details lifecycle, navigation, editing, deletion, image controls, video, fullscreen, and keyboard behavior | `docs/subsystems/lightbox.md` |
| Settings controllers, exclusive operation runner, progress handling, scan and thumbnail actions, duplicate resolver, and confirmation flows | `docs/subsystems/settings-operations.md` |
| Destructive filesystem/database operations, duplicate identity, CSV semantics, backup archive format, restore staging, rollback, and recovery limits | `docs/subsystems/data-safety-and-portability.md` |
| Local setup, Bun and Cargo dependencies, Vite and TypeScript configuration, development profiles, builds, packaging, and diagnostics | `docs/development/setup-and-build.md` |
| Vitest, Rust unit and integration, backend workflow, and desktop E2E test layers, commands, fixtures, isolation, and troubleshooting | `docs/development/testing.md` |
| Supported languages, runtime selection, locale key and placeholder invariants, adding translations, generator limitations, and validation | `docs/development/localization.md` |
