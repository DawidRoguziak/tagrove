# 0033: U+FEFF is tag whitespace in Rust, and legacy rows that contain it are split on read, not migrated

- Status: Accepted
- Date: 2026-11-13
- Areas: tags, CSV import and export, backup restore, database
- Related: [search, tags and media groups](../subsystems/search-tags-and-media-groups.md#tag-mutations), [data safety and portability](../subsystems/data-safety-and-portability.md), [database](../subsystems/database.md), commits `7a452e7` and `edd1f95` (decided and committed on 2026-11-11; this ADR records the decision afterwards)

## Context

A tag may not contain whitespace. The frontend decides that with JavaScript `\s` and `trim()` (`src/utils/media.ts`), which count U+FEFF (BOM, zero-width no-break space) as whitespace. The backend used `char::is_whitespace`, which does not. The two rules disagreed on that one character, and the backend is authoritative:

- A CSV tags field containing U+FEFF (for example a stray BOM inside a cell) was not split on it, so Rust stored a tag such as `ca<U+FEFF>t`.
- The frontend then treated that stored tag as invalid. `normalizeTags` dropped it from the canonical base, so the next lightbox edit silently removed the tag.

Fixing the rule leaves rows that earlier versions already wrote. They exist in live databases and in backup bundles, and an `init_schema` pass cannot reach bundle contents. Once Rust rejects U+FEFF, any flow that re-validates the stored tags of such an asset would fail: a bulk merge, a CSV import over that asset, a CSV export (its completed file is parsed with the same validators) and backup validation.

## Options considered

1. **Do nothing.** The disagreement stays: CSV import keeps writing tags that the frontend cannot represent, and the data silently diverges between the two layers. Rejected.
2. **Make U+FEFF whitespace in Rust, rewrite stored rows in `init_schema`.** Splitting a stored tag means creating tags, remapping assets, merging duplicates, repairing counters and `tag_count`, and bumping the library revision, for a character that is rare in practice. `init_schema` runs on every start, takes no backup and has no migration runner, and restore validates a bundle before it reaches any migration. Rejected: the risk is out of proportion to the data it touches, and the rows can be made harmless without rewriting them.
3. **Make U+FEFF whitespace in Rust and reject legacy rows.** Backup validation would refuse a bundle that the previous version produced, and a CSV export or bulk merge would abort on an asset the user cannot repair through the UI. Violates the rule that a valid earlier backup stays restorable ([ADR 0015](0015-restore-tolerates-unresolved-thumbnails-and-nullable-file-name.md) for the same stance). Rejected.
4. **Make U+FEFF whitespace in Rust and tolerate legacy rows on read (chosen).** The write rule matches the frontend, nothing stored is rewritten, and every flow that reads legacy rows normalizes them instead of failing.

## Decision

- `src-tauri/src/utils/tags.rs`: `is_tag_space` is `char::is_whitespace` plus U+FEFF. It drives `is_invalid_tag_character`, so U+FEFF is rejected inside a tag, and the edge trim of `normalize_tags` and `parse_csv_tags`, so it is trimmed at a tag's edges. `parse_csv_tags` and `parse_legacy_tags` split on it, so a CSV tags field becomes separate tags.
- `split_legacy_feff` splits stored tags on U+FEFF and normalizes the result. It is applied where stored tags feed validation: `merge_tags` (multi-asset bulk merge by ID), CSV import over an asset's existing tags (`db/mutations.rs`, `import_csv_record_batches`) and CSV export (`services/csv_service.rs`). Those flows complete, and the asset's tags are written in the split form where they write tags. Every other invalid character still fails.
- Backup validation (`db/schema.rs`, `validate_backup_data`) removes U+FEFF from a stored tag name before the canonical-name check and skips a name made only of U+FEFF. The row itself is restored as stored. Everything else about the name must still be canonical.
- No schema change and no data migration. A legacy row stays in `tags` until the user replaces the asset's tags: the frontend never shows a tag with an inner U+FEFF, so the next single-asset replacement (`set_asset_tags`) drops it. A merge over a ranged selection (`apply_selection_mutation`) leaves legacy rows as stored.
- The two layers share one vector table: `FEFF_VECTORS` in `utils/tags.rs` and the matching case in `src/utils/__tests__/media.test.ts` (edge U+FEFF trimmed, inner U+FEFF invalid, a lone U+FEFF empty).

## Consequences

- The Rust and frontend tag rules agree on U+FEFF, and a new tag written by any path cannot contain it. A CSV field `cat<U+FEFF>dog` imports as two tags.
- Legacy rows are not repaired. A tag with an inner U+FEFF can remain in the vocabulary and in tag counts until its assets are edited one at a time, and it is invisible in the editors. A CSV export writes it as separate tags, so an export followed by an import reaches the split form for those assets.
- Restoring a bundle from an earlier version that holds such a tag succeeds, and the restored database holds the same row. Backup validation is the only place that accepts a non-canonical tag name; a name that is non-canonical for any other reason is still rejected.
- Pinned by tests: the shared vectors (`feff_is_trimmed_at_the_edges_and_invalid_inside_like_the_frontend`, `treats U+FEFF like the backend`), `backup_validation_accepts_a_legacy_tag_name_containing_u_feff` (including the U+FEFF-only name), `bulk_tag_merge_splits_a_legacy_u_feff_tag_instead_of_failing`, `import_over_an_asset_with_a_legacy_u_feff_tag_is_not_rejected`, `export_writes_a_legacy_u_feff_tag_as_separate_tags` and `parser_treats_u_feff_in_a_tags_field_as_a_separator`.
- A future cleanup that rewrites the rows would need its own ADR superseding this one. Any further character on which JavaScript and Rust disagree about whitespace would extend `is_tag_space` and the shared vectors.
