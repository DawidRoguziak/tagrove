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
- Bulk selection is a spec (ADR 0008): session ranges plus small `excluded`/`included` ID maps. Ranges ≤ 10,000 positions resolve to IDs at once and survive filters; larger ranges are never materialized in the frontend, are written through `apply_selection_mutation` (all or nothing, `stale` on a changed session) and are cleared with a notice when the session changes.
- A spec write takes the library-wide queue turn (`enqueueLibraryMutation`) and drops every cached tag list and detail; never send a spec write outside that turn.
- If you change copy or aria labels, update the E2E selectors and the verify-mediatagger recipes.
