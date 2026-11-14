---
paths:
  - "src-tauri/src/indexer.rs"
  - "src-tauri/src/commands/scan.rs"
  - "src-tauri/src/usecases/scan.rs"
  - "src-tauri/src/adapter/startup_scan.rs"
  - "src-tauri/src/services/scan_service.rs"
  - "src-tauri/src/db/scan_membership.rs"
  - "src-tauri/src/utils/paths.rs"
---

# Scanning and indexing

Read `docs/subsystems/scanning-and-indexing.md`, especially its change checklist.

- Stale rows are pruned only for a root that scanned completely. A partial root keeps its old rows. Every new warning or failure source must either make the root partial, be documented as optional metadata, or be a discovery skip of a path that can never have a row (non-UTF-8 paths, ADR 0025); skips are logged, never change completeness.
- Overlapping roots share one `assets` row with several mappings. Removing a root deletes only globally orphaned assets, tags, and failures.
- Keep one stable path-identity policy across normalization, upserts, lookups, mappings, and frontend path conversion.
- Fingerprint changes are cache-invalidation changes. Keep thumbnail and failure versioning aligned.
- Kind detection uses the extension only. Scans serialize under the scan lock; the combined order is scan, then exclusive thumbnail lock.
- Keep progress phases and result fields in sync with `src/platform/tauri/contracts/scanning.ts`.
