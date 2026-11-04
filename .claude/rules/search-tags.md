---
paths:
  - "src/features/search/**"
  - "src/features/tagging/**"
  - "src/features/bulk/**"
  - "src/utils/**"
  - "src/library/metadata/**"
  - "src/library/vocabulary/**"
  - "src-tauri/src/db/mutations.rs"
  - "src-tauri/src/utils/tags.rs"
---

# Search, tags, bulk, media groups

Read `docs/subsystems/search-tags-and-media-groups.md` (it covers search grammar, selection gestures, and tag-write semantics).

- Search has separate draft and applied state. Only a submit applies filters. Operators `tags`, `tags:N`, and `gN:value` must be the only token. Quotes make a literal. The backend re-normalizes every filter.
- Tag rule (frontend `src/utils/media.ts` and backend `utils/tags.rs`): trimmed, lowercased, and no whitespace, comma, semicolon, or control characters. The backend decides.
- With one selected asset, edits replace its tags (`set_asset_tags`). With several, edits are add-only (`merge_asset_tags_bulk`).
- After a successful write, publish the canonical returned tags. Partial bulk results publish only acknowledged IDs.
- Bulk selection stores authoritative IDs independently of page eviction and filters.
- If you change copy or aria labels, update the E2E selectors and the verify-mediatagger recipes.
