# 0010: `zip` 9 with the Deflate backend only

- Status: Accepted
- Date: 2026-11-07
- Areas: dependencies, backup and restore, data safety
- Related: [data safety and portability](../subsystems/data-safety-and-portability.md), [plan stage 10](../plans/grid-performance/10-open-topics.md) (open topic 6)

## Context

`bun run quality` failed on its dependency audits, also on a clean HEAD:

- `audit:js`: zod 4.4.3, GHSA-mrhx-c7xc-m3w5 (high, fixed in 4.6.6).
- `audit:rust`: `zip` 2.4.2 (RUSTSEC-2026-0367/0368/0369/0429, fixed only in `zip` ≥ 9.0.0) and `zstd` / `zstd-safe` / `zstd-sys` (RUSTSEC-2026-0391/0398/0403/0410), which only `zip`'s default features pulled in.

The `zip` advisories concern `extract`, which the application never calls: `backup_service.rs` reads entries one by one through `by_index` with its own path, size, ratio and symlink checks. Still, a failing audit hides new findings, and the crate is the backup format's only reader. The backup writer has always used `CompressionMethod::Deflated` for every entry, so no bundle written by this application uses another method.

## Options considered

1. **Ignore the advisories in `audit:rust`.** No code change, but keeps a major-version-old archive reader and zstd/bzip2/lzma/AES code that nothing uses. Rejected.
2. **`zip` 9 with default features.** Fixes the `zip` advisories, keeps every codec. The `zstd` advisories stay unless `zstd` is also upgraded. Rejected: more code than needed.
3. **`zip` 9 with `default-features = false, features = ["deflate-flate2"]` (chosen).** Reads and writes Deflate (and stored entries) through the existing `flate2` backend; aes, bzip2, deflate64, lzma/xz, ppmd, zopfli and zstd leave the build.

## Decision

- `src-tauri/Cargo.toml`: `zip = { version = "9.4", default-features = false, features = ["deflate-flate2"] }`, locked at 9.4.1. `cargo update -p zip` changed only `zip` and its own dependencies: 23 crates removed, `typed-path`, `nix` 0.31 and `cfg_aliases` added.
- `zip` 9 decodes entry names fallibly (`ZipFile::name()` returns `Result`). Backup validation propagates the error, so an archive with an undecodable entry name is rejected before anything is extracted. `ZipFile` now carries its reader type (`ZipFile<'_, fs::File>`).
- `package.json`: `zod` `^4.6.6` (locked 4.6.7); only the `zod` entry of `bun.lock` changed.

## Consequences

- `bun run quality` passes, including `audit:js` and `audit:rust` (13 informational "unmaintained"/"unsound" warnings remain, in transitive dependencies of Tauri and its GTK stack, unchanged by this decision).
- A bundle compressed with another method (for example re-packed by an external tool with zstd or bzip2) is now rejected as unreadable instead of being restored. Bundles written by the application are Deflate and unaffected.
- Restoring a bundle written by the previous `zip` 2 build was not tested separately; both versions write standard Deflate entries with ZIP64 headers, and the backup round-trip and validation tests (`backup_service.rs`) pass on `zip` 9.
