# QA scenario catalog

The click-through catalog for the `/audit` QA stages (process: [README](README.md); driving
rules and traps: [qa-driving.md](../../../.claude/skills/audit/references/qa-driving.md); safety:
[safety.md](../../../.claude/skills/audit/references/safety.md)). The QA planner turns it into
`qa/plan.json`; each `QA-NN` suite runs as one `qa-tester` agent in the block's session, and
targeted `QA-T*` suites come from static records.

Notation used throughout:

- `ctl <cmd> <id> ...` stands for `bun run app:control <cmd> <id> ...`; type the full command
  (the QA hook allows only that form). Blocks S2–S4 omit `<id>` after the command for brevity.
- `<id>` is the literal session ID from the brief, `<run>` is `docs/review/<run>`, `<scratch>`
  is the session's scratch root (doctor prints `<scratch>/media` as the only scan root), and
  `<app-data>` is `<scratch>/data/com.example.mediatagger.e2e`.
- `DB` is `sqlite3 -readonly <app-data>/media.db`. Tables: `assets` (`kind` is `image`, `gif` or
  `video`; `modified_at` in seconds; `thumb_path`; `is_favorite`; `media_group_key`,
  `media_group_order`; `tag_count`), `tags`, `asset_tags`, `scan_roots`
  (`auto_scan_on_startup`), `thumbnail_failures`, `pending_file_operations`, `library_metadata`.
- Selectors are WebdriverIO syntax. `button=Rescan all` matches a button by its text, for buttons
  without an `aria-label` or test ID. Tiles are `button[data-asset-index="N"]` (with
  `data-asset-id`); unloaded positions are `aria-hidden` placeholders with the same index.
- File changes in the copy go only through `python3 .claude/skills/audit/scripts/qa_fs.py --run <run> ...`,
  with paths relative to `<scratch>/media`.
- Mark every suite with `python3 .claude/skills/audit/scripts/qa_session.py --run <run> mark --block <B> --suite <QA-NN> begin`
  before its first step and `... end` after its last.
- A scenario that cannot be provoked through the UI is recorded in the suite report under
  `scenarios_skipped` with the reason; that is a coverage gap, not a bug.

## Block S1: scale

S1 runs on a copy of every top-level folder of `$AUDIT_DATASET` except `for-claude-tests`: about 30k images in three photo folders, large images in `source/`, five long videos and empty folders. The tester model is Sonnet (agent role `qa_tester`). QA-01 to QA-08 share one S1 session; QA-09 runs in its own session built with `VITE_MEDIATAGGER_PERF=1`.


### QA-01 First scan from the UI

Destructive: no. Session: shared S1 session

**QA-01.1 Start the first scan and read progress**
- **Steps:** `ctl doctor <id>`; `ctl screenshot <id>`. Note the gallery state (empty or populated). Open settings (`ctl click <id> '#open-settings-button'`), `ctl click <id> 'button=Rescan all'` (or let the startup scan run if the root is attached). Record the wall-clock start from `actions.jsonl`. Every 30 s run `ctl inspect <id> '#settings-scan'` and note the progress text (`Counting files n/m`, `Scanning n/m`, `Cleanup n/m`).
- **Expected:** Progress texts follow counting, scanning, cleanup and end with `Done. Indexed: N, removed: 0, failed: 0` ([scanning](../../subsystems/scanning-and-indexing.md#progress-and-results)). `N` equals the number of supported files in the copy.
- **Oracle:** `sqlite3 -readonly <app-data>/media.db 'select count(*) from assets'` equals N; scan duration from `actions.jsonl` and `process.log` perf lines is recorded in the suite notes.

**QA-01.2 Gallery stays responsive during the scan**
- **Steps:** Go Back (`ctl click <id> 'button[aria-label="Back"]'`) while the scan runs. Within 5 s of every action: `ctl wheel <id> .gallery-scroll 1400`, `ctl keys <id> End`, `ctl keys <id> Home`, then `ctl click <id> 'button[data-asset-index="0"]'` and close with `ctl keys <id> Escape`.
- **Expected:** Scrolling and the lightbox respond while the scan is running; the settings operation continues after returning to the gallery ([settings operations](../../subsystems/settings-operations.md#application-and-view-lifetimes)). No frozen frame longer than a few seconds.
- **Oracle:** `ctl inspect <id> 'button[data-asset-index="0"]'` after each action; timestamps of consecutive `actions.jsonl` lines; `console.jsonl` has no errors or unhandled rejections.

**QA-01.3 End state and counter**
- **Steps:** After the scan reports `Done`, `ctl wait <id> idle 90000`, then `ctl keys <id> End` and `ctl inspect <id> '[data-testid="gallery-end-state"]'`.
- **Expected:** The footer shows `End of results` and the locale-formatted total (for example `30,000 items`) equal to the number of indexed assets ([gallery states](../../subsystems/library-query-and-gallery.md#gallery-presentation-states)). There is no `Generating thumbnails…` footer and no `role=status` on it.
- **Oracle:** Footer number equals `select count(*) from assets`; `ctl screenshot <id>` at the end of results.

**QA-01.4 Thumbnails appear and `thumb://` 503 retries are logged**
- **Steps:** At the top of the gallery run `ctl wait <id> '[data-testid="tile-thumb-loader"]' hidden 60000` (the loader is always in the DOM; wait for `hidden`, not `gone`). Scroll to a not-yet-seen part with `ctl wheel <id> .gallery-scroll 5000` three times. Search `process.log` and `console.jsonl` for `thumb://` entries with status 503 or 404.
- **Expected:** Tiles show pictures. A 503 (queue full or maintenance, `Retry-After: 1`) is retried after 500 ms and 2000 ms and then resolves; a tile ends as a transparent placeholder only after the retries are exhausted ([thumbnails](../../subsystems/thumbnails.md#the-thumb-protocol), [frontend tiles](../../subsystems/thumbnails.md#frontend-tiles)). A 404 with `c=1` is only a cache miss.
- **Oracle:** Cite the log lines with timestamps. Count of 503 lines, and whether any tile in the final screenshot is blank. `sqlite3 -readonly <app-data>/media.db 'select count(*) from assets where thumb_path is not null'` rises over time; `select count(*) from thumbnail_failures` stays explained.

**QA-01.5 Console and process log after the scan**
- **Steps:** `ctl logs <id>`; read `console.jsonl` and `process.log` for the whole scan window.
- **Expected:** No `console.error`, window errors, unhandled rejections or Rust panics. Warnings are listed, not ignored.
- **Oracle:** Entries with timestamps; classify each as expected or suspect.

### QA-02 Gallery at scale

Destructive: no. Session: shared S1 session

**QA-02.1 Scroll to start, middle and end with keys and wheel**
- **Steps:** `ctl keys <id> End`, `ctl wait <id> '[data-testid="gallery-end-state"]'`, `ctl keys <id> Home`. Then `ctl keys <id> PageDown` 20 times, `ctl wheel <id> .gallery-scroll 3000` and `-3000`. Jump to the middle with the scrubber (QA-02.7) or repeated wheel steps. At each position `ctl screenshot <id>`.
- **Expected:** Tiles exist at every position (`button[data-asset-id][data-asset-index]`); not-yet-loaded indices are `aria-hidden` placeholders that become tiles when their page arrives ([sparse loading](../../subsystems/library-query-and-gallery.md#virtual-range-loading-and-thumbnail-prefetch)). Home/End reach the real first and last item.
- **Oracle:** `ctl inspect <id> 'button[data-asset-index]'` lists indices; after End, the last tile has `data-asset-index` = total - 1; screenshots show no empty grid after the loader clears.

**QA-02.2 Fast scroll sets `data-scroll-paused`**
- **Steps:** `ctl wheel <id> .gallery-scroll 5000` followed at once by `ctl inspect <id> '[data-testid="gallery-grid"]'`; repeat with `ctl keys <id> End`. After 300 ms inspect again.
- **Expected:** A single jump of at least half a viewport (or a viewport within 100 ms) sets `[data-scroll-paused]` for about 150 ms, then it clears. Tiles that were loaded stay visible with pictures; new tiles shown during the pause ask only for cached thumbnails (`c=1`) and fetch normally afterwards ([fast scrolling](../../subsystems/library-query-and-gallery.md#virtual-range-loading-and-thumbnail-prefetch)).
- **Oracle:** `inspect` shows the attribute right after the jump and not 300 ms later (timing through the `actions.jsonl` timestamps; if the 150 ms window is missed, repeat and report the frequency x/y). `thumb://...&c=1` lines in `process.log` for the new tiles; the final screenshot has no permanently blank tiles.

**QA-02.3 Tile size slider**
- **Steps:** `ctl inspect <id> '#tile-size'` (min, max, step, value). `ctl fill <id> '#tile-size' 140`, then `340`, then `188` and `keys` ArrowRight/ArrowLeft while the slider is focused. Open settings and return.
- **Expected:** Range is 140-340, step 8, default 188 on a fresh start. The grid column count changes and the first visible item stays in view ([anchor](../../subsystems/library-query-and-gallery.md#virtual-range-loading-and-thumbnail-prefetch)). The value is not persisted (verified after `ctl refresh <id>`: back to 188).
- **Oracle:** `inspect '#tile-size'` attributes and value; `inspect 'button[data-asset-index]'` after each change shows the same first visible index; screenshot at 140 and 340.

**QA-02.4 Ctrl+wheel zoom**
- **Steps:** From 188: `ctl wheel <id> .gallery-scroll -8 ctrl` three times, then `ctl wheel <id> .gallery-scroll 8 ctrl` six times, then 60 more steps in one direction.
- **Expected:** Each step changes the size by 8 px and the value clamps at 140 and 340. Plain wheel never zooms ([scrubber/zoom note](../../subsystems/library-query-and-gallery.md#timeline-sections-and-scrubber)).
- **Oracle:** `inspect '#tile-size'` value after each series (188 -> 212 -> 164 and so on, clamped).

**QA-02.5 Media-kind filter**
- **Steps:** `ctl select <id> '#gallery-media-kind' value=image`, then `gif`, `video`, `all`. After each, `ctl wait <id> idle`, `End`, `inspect '[data-testid="gallery-end-state"]'`.
- **Expected:** Footer count changes per kind; Video shows 5 items (the five long videos) plus any other videos, GIF only GIFs; All returns the full total. Kind chips `Video` and `GIF` are visible on the matching tiles ([presentation](../../subsystems/library-query-and-gallery.md#gallery-presentation-states)).
- **Oracle:** `sqlite3 -readonly <app-data>/media.db "select kind,count(*) from assets group by kind"` (kinds `image`, `gif`, `video`) matches each footer count.

**QA-02.6 Empty folders and the empty state**
- **Steps:** Check that empty copied folders add no assets (compare the folder list under `<scratch>/media` with the DB paths). `ctl fill <id> .filter-input zzzz-no-such-tag`, `ctl keys <id> Enter`, `ctl inspect <id>`.
- **Expected:** Empty folders add nothing and cause no error. The no-match query shows `No results` with the hint to change filters; the add-folder action appears only when no folders are attached ([states](../../subsystems/library-query-and-gallery.md#gallery-presentation-states)). No footer is shown on an empty result.
- **Oracle:** `inspect` text; screenshot; `ctl click <id> 'button[aria-label="Clear all search filters"]'` restores the full gallery.

**QA-02.7 End of results footer**
- **Steps:** `ctl keys <id> End` in All and in the Video filter; `ctl inspect <id> '[data-testid="gallery-end-state"]'`; scroll one row up and check that the footer leaves the view.
- **Expected:** `End of results` plus a pluralized count (`1 item` is not expected here; 5 videos give `5 items`); the footer scrolls with the gallery and does not cover the last row.
- **Oracle:** Inspect text and screenshot; last tile fully visible.

**QA-02.8 Load error and Retry**
- **Steps:** Provoke a load failure only through a supported action, for example the maintenance window of an import is S2 territory and is not allowed here. If the failure cannot be provoked (the expected case), record the gap and run `ctl inspect <id> '[data-testid="gallery-load-error"]'` to prove the banner is absent.
- **Expected:** On failure a `role=alert` banner `[data-testid="gallery-load-error"]` with `Loading the library failed.` and a `Retry` button appears; Retry reissues failed pages and the selection read ([error behavior](../../subsystems/library-query-and-gallery.md#error-behavior)).
- **Oracle:** If shown: `inspect`, click `button=Retry`, the banner disappears and the grid reloads. If not provoked: list as gap `load-error-not-provokable-via-ui` (covered by Vitest).

**QA-02.9 Timeline scrubber: visibility and aria**
- **Steps:** `ctl inspect <id> '[data-testid="gallery-timeline"]'`. If it is absent, results span one month: spread modification times with `python3 .claude/skills/audit/scripts/qa_fs.py --run <run> touch dataset/<folder> --days-ago 60` for two or three sub-folders (different N, for example 40, 120, 400), then Settings -> `button=Rescan all`, Back, `wait idle`. Inspect `[role="slider"][aria-label="Timeline"]` (aria-orientation vertical, `aria-valuetext` = month name, or `No date`).
- **Expected:** The scrubber is shown only when results span at least two months; `aria-valuetext` is the month (UTC, UI language) of the first visible item; years are labelled without overlap ([scrubber](../../subsystems/library-query-and-gallery.md#timeline-sections-and-scrubber)).
- **Oracle:** `inspect` attributes; `sqlite3 -readonly <app-data>/media.db "select strftime('%Y-%m',modified_at,'unixepoch'),count(*) from assets group by 1"` (`modified_at` is in seconds) shows more than one month.

**QA-02.10 Timeline scrubber: click, drag, hover label**
- **Steps:** `ctl click <id> '[data-testid="gallery-timeline"]'` (mid-track), `inspect` the gallery and `aria-valuetext`. `ctl drag <id> '[data-testid="gallery-timeline"]' '[data-testid="gallery-timeline"]@0,120'` and inspect `[data-gallery-timeline-dragging]` and `[data-testid="gallery-timeline-label"]` during and after the drag.
- **Expected:** A click jumps to the proportional index; a drag applies one jump per frame, `data-gallery-timeline-dragging` is set during the drag, pages and thumbnails resume on release or after 150 ms without movement. The label shows the month under the pointer.
- **Oracle:** `inspect` of `aria-valuetext` and the first visible `data-asset-index` before and after; screenshot during the drag; no console errors.

**QA-02.11 Timeline scrubber: keyboard**
- **Steps:** Focus the slider (`ctl click <id> '[data-testid="gallery-timeline"]'`), then `ctl keys <id> ArrowDown`, `ArrowUp` twice, `PageDown`, `PageUp`, `End`, `Home`, inspecting `aria-valuetext` after each. Repeat with the chords `Alt+ArrowDown`, `Control+ArrowDown`, `Meta+ArrowDown`.
- **Expected:** Down/Up move one month (Up first returns to the start of the current month), PageDown/PageUp one year (to the start of the current year first), Home/End the ends. Alt, Ctrl and Meta chords are ignored ([scrubber](../../subsystems/library-query-and-gallery.md#timeline-sections-and-scrubber)).
- **Oracle:** `aria-valuetext` sequence and first visible index recorded per key; unchanged after the modified chords.

**QA-02.12 Gaps: windowed mode and position bar**
- **Steps:** `ctl inspect <id> '[data-testid="gallery-scrollbar"]'` and `.gallery-scroll[data-gallery-windowed]` at the end of the gallery with tile size 140.
- **Expected:** Neither exists at about 30k items (grid height stays under 16,000,000 px). This is a gap, not a bug; the behaviour is covered by the `GALLERY_100K` sweep and the `gallery-scale` harness ([windowed mode](../../subsystems/library-query-and-gallery.md#virtual-range-loading-and-thumbnail-prefetch)).
- **Oracle:** Both selectors absent; record gap `windowed-mode-unreachable-at-30k` in the suite notes.

### QA-03 Search

Destructive: no. Session: shared S1 session

**QA-03.1 Tag grammar: AND, exclusion, case**
- **Steps:** Pick two real tags from the tag list (`ctl click <id> 'button[aria-label="Open tag list"]'`, `inspect '[data-testid="tag-list-modal"]'`, close with Escape). `ctl fill <id> .filter-input '<a> <b>'`, `Enter`; then `'<a> -<b>'`; then `'<A> <B>'` in upper case; then a trailing/duplicate space `'  <a>   <a> '`.
- **Expected:** Spaces mean AND, `-tag` excludes, matching is case-insensitive, tokens are de-duplicated ([grammar](../../subsystems/search-tags-and-media-groups.md#search-grammar-and-filter-semantics)).
- **Oracle:** Footer count per query equals `sqlite3 -readonly <app-data>/media.db` join over `tags` and `asset_tags` (`select count(distinct a.asset_id) from asset_tags a join tags t on t.id=a.tag_id where t.name='<tag>'`).

**QA-03.2 Metatags `tags`, `tags:N`, `gN:`**
- **Steps:** `tags`, `tags:0`, `tags:2`, `tags:007`, `gN:trip` (use a real group key from QA-07), `gN:Trip 2026` (rest of the line is the value).
- **Expected:** `tags` means exactly 0 tags; `tags:N` exact count N (leading zeros accepted); `gN:` is case-insensitive and keeps inner spaces.
- **Oracle:** Footer count equals `select count(*)` of assets with the matching `tag_count` or `media_group_key_normalized`.

**QA-03.3 The four validation errors**
- **Steps:** Submit each: `tags dog` (or `cat gN:x`) -> solo error; `tags:abc`, `tags:-1`, `tags:1.5`, `tags:` -> integer error; `gN:` -> missing value; `bad,tag`, `a;b`, `"unclosed` -> invalid characters. After each `ctl inspect <id>` and `ctl screenshot <id>`.
- **Expected:** Messages: `Metatags must be used on their own, without other tags.`, `tags accepts only a non-negative integer value.`, `gN requires a group value after the colon.`, `Tags cannot contain whitespace, commas, semicolons, or control characters.` The previously applied results stay unchanged ([draft vs applied](../../subsystems/search-tags-and-media-groups.md#draft-filters-applied-filters-and-refreshes)).
- **Oracle:** Visible error text per input; footer count unchanged; no IPC call (no new query line in `process.log` for the invalid submit).

**QA-03.4 Quotes and escapes**
- **Steps:** Submit `"tags"`, `"tags:3"`, `"-holiday"`, `-"-holiday"`, `"a\"b"`, `"a\\b"`.
- **Expected:** Quoted names are literal tags, not operators; `-"x"` excludes the literal; `\"` and `\\` are literal characters. Unknown tags give `No results`.
- **Oracle:** Result counts; the input text after Apply from the tag list re-serializes names beginning with `-` in quotes ([serializeTagToken](../../subsystems/search-tags-and-media-groups.md#search-grammar-and-filter-semantics)).

**QA-03.5 Ctrl+K, suggestions, Enter behavior**
- **Steps:** Click the gallery body, `ctl keys <id> Control+k`, `ctl inspect <id> ':focus'` (or `.filter-input` active state). Type a prefix with `ctl fill <id> .filter-input <prefix>`; `inspect` the suggestion listbox (`[role="listbox"]`, at most 8 options). `ctl keys <id> ArrowDown ArrowUp Escape`. Type again and press `ctl keys <id> Enter` once, inspect the input, then `Enter` a second time.
- **Expected:** Ctrl+K focuses the search; suggestions open only while focused with a non-empty value, arrows wrap, Escape closes. The first Enter inserts the first suggestion, the second submits ([autocomplete](../../subsystems/search-tags-and-media-groups.md#search-autocomplete)). Assigned/used tags are not suggested again.
- **Oracle:** Input value after each Enter; option count <= 8; footer count after the second Enter matches the DB.

**QA-03.6 Clear all search filters**
- **Steps:** Set text, media kind `video` and the favorites heart (`ctl click <id> 'button[aria-label="Show favorites only"]'`), then `ctl click <id> 'button[aria-label="Clear all search filters"]'`.
- **Expected:** Text, kind (All) and favorites all reset and the full gallery returns.
- **Oracle:** `inspect .filter-input`, `inspect '#gallery-media-kind'`, heart `aria-pressed="false"`, footer = full total.

**QA-03.7 Trap: invalid text plus filter change applies nothing**
- **Steps:** Type `tags:abc` without losing the previous valid state (apply `<a>` first). Then change `#gallery-media-kind` to `video` and toggle the favorites heart.
- **Expected:** With invalid text in the draft, changing kind or favorites applies nothing (the previous result stays; this is documented behaviour, record it as the trap). After correcting the text and submitting, all pending draft values apply together.
- **Oracle:** Footer count unchanged after each change, then changed after the corrected submit; `process.log` shows no query start in between.

**QA-03.8 Long and unicode queries**
- **Steps:** Submit a 2,000-character string of 100 distinct tokens; a single 500-character token; `ł ą ż ć 日本語 😀`; mixed case `ŁÓDŹ`; a token with a combining accent.
- **Expected:** No hang or crash; long inputs either match (no results) or show a validation error; unicode is lower-cased and matched like other tags; the UI layout does not break (no horizontal overflow).
- **Oracle:** Responsiveness (next action within a few seconds), screenshot, `console.jsonl` clean; no tags with these characters are created by searching.

**QA-03.9 Suggestions retry**
- **Steps:** If `ctl inspect <id>` shows `Suggestions unavailable. Retry` (the worker failed), click `button=Suggestions unavailable. Retry`. Otherwise record `not-provokable`.
- **Expected:** Retry refocuses the input and reopens suggestions ([autocomplete](../../subsystems/search-tags-and-media-groups.md#search-autocomplete)).
- **Oracle:** Listbox appears after Retry; otherwise gap noted.

### QA-04 Tagging and the tag vocabulary

Destructive: no. Session: shared S1 session

**QA-04.1 Normalization in the lightbox**
- **Steps:** Open `ctl click <id> 'button[data-asset-index="0"]'`. `ctl fill <id> '#lightbox-tag-draft-input' '  MixedCase  '`, `ctl keys <id> Enter`. Add `mixedcase` again, then `Another`.
- **Expected:** Tags are trimmed and lowercased; duplicates are ignored; the chip list shows `mixedcase`, `another` ([tag mutations](../../subsystems/search-tags-and-media-groups.md#tag-mutations)). The save is immediate.
- **Oracle:** `inspect '[data-testid="lightbox-tag-panel"]'`; `sqlite3 -readonly <app-data>/media.db` tags of the asset (join over `asset_tags`; confirm schema).

**QA-04.2 Invalid characters keep the draft (lightbox)**
- **Steps:** `fill '#lightbox-tag-draft-input' 'a b'` + Enter; then `a,b`, `a;b`, and one with a control character through `fill` (`a\u0007b` where supported).
- **Expected:** `[data-testid="lightbox-tag-invalid"]` shows `Tags cannot contain whitespace, commas, semicolons, or control characters.`; the draft stays in the input and no write happens.
- **Oracle:** `inspect '[data-testid="lightbox-tag-invalid"]'`; input value unchanged; DB tags unchanged; no `set_asset_tags` line in `process.log`.

**QA-04.3 Invalid characters keep the draft (bulk)**
- **Steps:** Close the lightbox, `ctl click <id> 'button[aria-label="Enable bulk actions"]'`, click tile 0 and 1 (`button[data-asset-index="0"]`), `fill '#bulk-tag-draft-input' 'x y'` + Enter.
- **Expected:** `[data-testid="bulk-tag-invalid"]` is shown, the draft remains, nothing is written.
- **Oracle:** `inspect '[data-testid="bulk-tag-invalid"]'`, `inspect '#bulk-tag-draft-input'`; DB unchanged.

**QA-04.4 Suggestions exclude assigned tags**
- **Steps:** With the lightbox open on an asset that has `mixedcase`, `fill '#lightbox-tag-draft-input' mixed`; `inspect '[role="listbox"]'`. Repeat in bulk with one selected asset.
- **Expected:** Assigned tags are not offered; other matching tags are, at most 8 ([autocomplete](../../subsystems/search-tags-and-media-groups.md#search-autocomplete)).
- **Oracle:** Option texts in `inspect`.

**QA-04.5 Orphan tags disappear**
- **Steps:** Add a unique tag `qa-orphan-01` to one asset only. Check it in the tag list and in suggestions. Remove its chip (`button[aria-label="Remove tag qa-orphan-01"]`). Reopen the tag list and type `qa-orphan` in `#tag-list-filter-input`.
- **Expected:** After removal the tag disappears from the list and from suggestions (no rename/merge/delete exists) ([tags](../../subsystems/search-tags-and-media-groups.md#tag-mutations)).
- **Oracle:** `inspect '[data-testid="tag-list-modal"]'` shows `No tags match this filter.`; `select count(*) from tags where name='qa-orphan-01'` returns 0.

**QA-04.6 Tag list: open, filter, load more by 100**
- **Steps:** `ctl click <id> 'button[aria-label="Open tag list"]'`, `inspect '[data-testid="tag-list-modal"]'` (count `li`), scroll the list with `ctl wheel <id> '[data-testid="tag-list-modal"] [role="list"]' 1500` (or the element that scrolls), `ctl fill <id> '#tag-list-filter-input' a` and clear it.
- **Expected:** The first page has 100 entries and the next 100 load within 120 px of the bottom while `items < total`; the filter is case-insensitive substring and resets the list to the top ([tag-list browser](../../subsystems/search-tags-and-media-groups.md#known-tags-and-the-tag-list-browser)).
- **Oracle:** `li` count 100 -> 200 -> ...; `select count(*) from tags` equals the eventual total; `Loading more tags...` visible during a page load.

**QA-04.7 Tag list: include, exclude, Apply**
- **Steps:** `ctl click <id> '[data-testid="tag-list-modal"] li button'` (wait 300 ms, inspect `aria-pressed` and the summary `Included: 1 | Excluded: 0`). `ctl dblclick` another chip (exclude). Focus a third chip and `ctl keys <id> Shift+Enter`. Click `button=Apply`. Reopen and check that the state starts empty.
- **Expected:** Single click toggles include after 220 ms; double click and Shift+Enter/Shift+Space set exclude (include -> exclude, exclude -> include); Apply replaces the whole search text with includes followed by `-` excludes, sorted case-insensitively, and closes the modal. Media kind and favorites drafts are unchanged.
- **Oracle:** `inspect .filter-input` text; footer count after Apply matches the DB for the same expression; `Applying...` shown while in flight.

**QA-04.8 Tag list: Escape and no mutation**
- **Steps:** Open the list, include a chip, press `Escape`, reopen.
- **Expected:** Closing resets query, selection and pending clicks; the search text is unchanged.
- **Oracle:** `inspect .filter-input`; summary `Included: 0 | Excluded: 0`.

### QA-05 Lightbox

Destructive: no. Session: shared S1 session

**QA-05.1 Open, wraparound at the 30k boundaries**
- **Steps:** `ctl keys <id> Home`, open tile 0 (`ctl click <id> 'button[data-asset-index="0"]'`), `ctl keys <id> ArrowLeft`, `inspect '[role="dialog"]'`. Close; `End`, open the last tile, `ctl keys <id> ArrowRight`.
- **Expected:** Left from the first goes to the last and Right from the last to the first (`(i +/- 1 + count) % count`); a transient `Finding this photo in the results...` may show ([navigation](../../subsystems/lightbox.md#global-navigation-and-library-synchronization)). Dialog title is `Preview <file name>`.
- **Oracle:** Dialog `aria-label` / `data-lightbox-kind`; the file name equals the DB row at the expected index (`order by` per [database](../../subsystems/database.md#gallery-and-media-group-ordering)); screenshot.

**QA-05.2 Hundreds of fast navigation steps**
- **Steps:** Hold the key by sending `ctl keys <id> ArrowRight` 300 times in a loop (`for i in $(seq 300); do ctl keys <id> ArrowRight; done`), then 300 times ArrowLeft. After the loop `ctl wait <id> idle 30000`, `inspect '[role="dialog"]'`.
- **Expected:** Presses accumulate on a target index, only the latest lookup selects; the final photo corresponds to +300 (mod total); no stuck `Finding this photo` message, no `Could not restore navigation`.
- **Oracle:** The final dialog name equals the asset at index (start + 300) % total in the DB; `console.jsonl` has no errors; memory sample in `qa/sessions/*-samples.csv` does not jump by gigabytes.

**QA-05.3 Large images from `source/`**
- **Steps:** Filter with a path-based tag is not available; locate large images by `sqlite3 -readonly <app-data>/media.db "select id,file_name,size_bytes from assets order by size_bytes desc limit 5"` and open the first via the lightbox by navigating (or search tags if they have one). `inspect '[data-testid="lightbox-image"]'`, wait for `[data-testid="lightbox-details-loading"]` to disappear.
- **Expected:** The large image displays fitted without `Could not display image`; zoom stays responsive; info panel shows dimensions and size.
- **Oracle:** `inspect '[data-testid="lightbox-image"]'` naturalWidth/Height plausible; load time from `actions.jsonl`; screenshot.

**QA-05.4 Zoom: click, wheel, keys, limits**
- **Steps:** Click the image (`ctl click <id> '[data-testid="lightbox-image"]'`) -> x1.25. `ctl wheel <id> '[data-testid="lightbox-image"]' -300` repeatedly to the maximum, then positive to 1. Keys: `ctl keys <id> =`, `+`, `-`, `0`.
- **Expected:** Zoom range x1-x12, discrete step x1.25; wheel zoom is anchored on the cursor and cannot scroll the page; `0` resets zoom and pan ([zoom](../../subsystems/lightbox.md#image-fit-zoom-and-pan)). Zoom below 1 is impossible.
- **Oracle:** `--lightbox-image-zoom` custom property and image size via `inspect '[data-testid="lightbox-image"]'`; the Reset button `button[aria-label="Reset zoom"]` appears when zoomed.

**QA-05.5 Pan and reset**
- **Steps:** Zoom to about x4, `ctl drag <id> '[data-testid="lightbox-image"]' 120,80`, inspect the transform, drag past the edge, then `ctl click <id> 'button[aria-label="Reset zoom"]'`. Navigate to the next photo.
- **Expected:** Panning is clamped to half the overflow per axis; the drag does not trigger a click-zoom (120 ms suppression); navigation resets zoom and pan.
- **Oracle:** Transform values before/after; screenshot.

**QA-05.6 Fullscreen F and the Escape queue**
- **Steps:** `ctl keys <id> f`, `inspect`, `ctl keys <id> Escape` (leaves fullscreen only), `Escape` again (closes). Then open the delete confirmation (`ctl click <id> '#lightbox-delete-button'`), press `Escape` (closes only the confirm), `Escape` (closes the lightbox). In a window narrower than 768 px (`ctl resize <id> 700 800`) open the drawer (`button[aria-label="Open asset panel"]`) and `Escape`.
- **Expected:** Escape order: delete confirm -> speed menu -> fullscreen -> drawer -> close. Nothing is deleted. Fullscreen toggles via F and the button `button[aria-label="Toggle fullscreen"]` ([keyboard](../../subsystems/lightbox.md#fullscreen-and-keyboard-behavior)). Under Xvfb the fullscreen state may not be provable; record a gap in that case.
- **Oracle:** Dialog presence after each Escape (`inspect '[role="dialog"]'`); `select count(*) from assets` unchanged. Restore the window with `ctl resize <id> 1440 900`.

**QA-05.7 Info panel**
- **Steps:** `ctl click <id> 'button*=Information'`, `inspect '[data-testid="lightbox-info-panel"]'`, repeat for a PNG, a large image and an asset with no dimensions.
- **Expected:** Type, size, dimensions and path are shown; the panel toggles (`aria-expanded`).
- **Oracle:** Values match `select kind,size_bytes,width,height,path from assets where file_name=...` (confirm columns).

**QA-05.8 Sidebar auto-hide and drawer**
- **Steps:** Open the lightbox, do nothing for 4 s, `ctl inspect <id> '#lightbox-sidebar'`, `ctl inspect <id> '[data-testid="lightbox-action-rail"]'`; press an unrelated key. Resize to 700x800 and check the drawer plus `[data-testid="lightbox-sidebar-scrim"]`.
- **Expected:** After 3 s of inactivity the controls hide; the first key press after inactivity reveals them and also executes its shortcut. Below 768 px the sidebar is a drawer with a scrim.
- **Oracle:** `inspect` opacity/pointer-events before and after; screenshot.

**QA-05.9 Rapid write series: favorite toggle reversal**
- **Steps:** In the lightbox click `button[aria-label="Add to favorites"]` three times quickly (`ctl click` three times with no waits), then `ctl wait <id> idle`. Repeat with 4 clicks.
- **Expected:** Writes go through the per-asset FIFO queue; a repeated click reverses the newest pending toggle, so odd number of clicks ends as favorite, even as not favorite; the button is `aria-busy` while pending ([favorites](../../subsystems/search-tags-and-media-groups.md#favorites-and-media-groups)).
- **Oracle:** Final heart label (`Remove from favorites` or `Add to favorites`) and `select is_favorite from assets where id=...`.

**QA-05.10 Rapid write series: tag queue and rebase**
- **Steps:** In one asset, `fill '#lightbox-tag-draft-input' t1`, Enter, `t2`, Enter, `t3`, Enter without waiting; immediately change the favorite and the group key (`#lightbox-media-group-key-input`, Apply). Then navigate away during the pending save and back.
- **Expected:** Edits are applied in order, each rebased on the current tags; nothing is rejected as busy; final tags are `t1,t2,t3`; `Saving tags...` is shown while pending and clears; no lost update.
- **Oracle:** `inspect '[data-testid="lightbox-tag-panel"]'` and the DB tags of that asset; `console.jsonl` has no rejections.

**QA-05.11 Load and error states**
- **Steps:** During normal use look for `[data-testid="lightbox-details-error"]`, `[data-testid="lightbox-media-error"]` and `button=Retry loading details`.
- **Expected:** They appear only on failure and offer Retry; absence under normal use is the pass condition. `Lazy chunk` failure (`The preview could not be loaded`) is not provokable here.
- **Oracle:** `inspect` of the selectors (absent); gap note for the error screens.

### QA-06 Selection and bulk actions

Destructive: no. Session: shared S1 session

**QA-06.1 Enable bulk mode and single selection**
- **Steps:** `ctl click <id> 'button[aria-label="Enable bulk actions"]'`; check `aria-pressed="true"`. `ctl click <id> 'button[data-asset-index="0"]'`, then `5`, then a Ctrl+click of 3 (`ctl modclick <id> Control 'button[data-asset-index="3"]'`) and of 0 again.
- **Expected:** A click toggles one tile; the panel `[data-testid="bulk-action-panel"]` shows `Selected: N`. Tiles have `aria-pressed`. Opening the lightbox does not happen in selection mode.
- **Oracle:** `inspect '[data-testid="bulk-action-panel"]'` for N after each step; tile `aria-pressed`.

**QA-06.2 Shift and Ctrl+Shift ranges across unloaded pages**
- **Steps:** Click tile 0, scroll far (`ctl keys <id> End`), `ctl modclick <id> Shift 'button[data-asset-index="<last visible>"]'`. Then click tile 10 in the middle, `ctl modclick <id> Control+Shift 'button[data-asset-index="<n>"]'`.
- **Expected:** Shift+click replaces the selection with the range anchor..target; Ctrl+Shift+click adds a range. The count equals the covered positions even though intermediate pages were never loaded ([selection](../../subsystems/search-tags-and-media-groups.md#bulk-selection)).
- **Oracle:** `Selected: N` equals the index span +1 (and the union for Ctrl+Shift).

**QA-06.3 Rectangle selection with auto-scroll**
- **Steps:** `ctl drag <id> 'button[data-asset-index="0"]' 400,300 200` (hold 200 ms), `inspect '[data-testid="gallery-selection-rectangle"]'` during if possible. Then a drag with hold 100 ms and a drag shorter than 5 px. Drag to the bottom edge (`<x>,<viewport bottom - 10>`) with hold 1000 and inspect the scroll offset. Repeat with Control held: `ctl drag <id> 'button[data-asset-index="20"]' 300,200 200 ctrl` adds to the selection.
- **Expected:** A rectangle needs at least 150 ms hold and 5 px movement; nearer than 48 px to the viewport edge it auto-scrolls up to 900 px/s; the scrollbar gutter is ignored. Shorter holds do not draw a rectangle ([rectangle](../../subsystems/library-query-and-gallery.md#bulk-rectangle-selection)).
- **Oracle:** `Selected: N` equals rows x columns covered; rectangle element present during the drag; `inspect` of the first visible index changes with auto-scroll.

**QA-06.4 Escape clears**
- **Steps:** With a selection, `ctl keys <id> Escape`, then `inspect '[data-testid="bulk-action-panel"]'`.
- **Expected:** The selection is cleared and bulk mode stays on; the hint `Esc clears selection` is shown in the panel.
- **Oracle:** `Selected: 0`; button `aria-pressed` still `true`.

**QA-06.5 Ctrl+A over 30k as one range (spec)**
- **Steps:** Bulk mode on, focus not in a text field, `ctl keys <id> Control+a`, `inspect '[data-testid="bulk-action-panel"]'`. Click the first tile with Ctrl to deselect one (an exclusion inside the range).
- **Expected:** One range `[0, total-1]` is selected without ID reads; `Selected: <total>`; with one exclusion `Selected: <total>-1`. Ctrl+A does nothing in a text field, outside selection mode, or when the lightbox/settings is on top ([ADR 0008](../../adr/0008-selection-spec-resolved-in-rust.md)).
- **Oracle:** The count equals the footer count; `process.log` shows no 30k-ID range read (no `get_asset_query_id_range` for the whole range). Try Ctrl+A with focus in `.filter-input`: the count must not change.

**QA-06.6 Range at most 10k versus over 10k**
- **Steps:** Select a 9,990-item range (Shift+click on index 9,989) and write a tag (`fill '#bulk-tag-draft-input' qa-small`, Enter). Then select a 10,100-item range and write `qa-big`.
- **Expected:** At or below 10,000 the range is resolved into IDs by one `get_asset_query_id_range`; above it stays a spec and is written by `apply_selection_mutation` all-or-nothing in one transaction ([bulk tags](../../subsystems/search-tags-and-media-groups.md#tag-mutations)).
- **Oracle:** `process.log` command names for each write; `select count(*)` of assets with each tag equals the range size (9,990 / 10,100); count of assets with a partial subset is 0.

**QA-06.7 Bulk tags: one item replaces, many merge**
- **Steps:** Select one asset that has tags: check the chips, remove one chip, add one. Select three assets with different tags: add `qa-merge`; check the chips of each afterwards in the lightbox.
- **Expected:** One item: complete replacement through `set_asset_tags` (chips removable). Several items: add-only; existing tags are kept and the new one is merged into every target; confirmation chips are non-removable ([tags](../../subsystems/search-tags-and-media-groups.md#tag-mutations)). `Most used tags` buttons add the same way.
- **Oracle:** DB tags per asset; `Updated X of Y` is shown only when part of the IDs could not be written.

**QA-06.8 Bulk favorites**
- **Steps:** Select 3 assets (not all favorites), `ctl click <id> 'button[aria-label="Toggle favorites for selected items"]'`; again (now all favorites) -> aria changes to `Remove selected items from favorites`. Repeat on a Ctrl+A range.
- **Expected:** Not all favorites -> all become favorites; all favorites -> all removed. A range uses the same all-or-nothing rule and re-reads the visible rows.
- **Oracle:** `select count(*) from assets where is_favorite=1` before/after; heart state in the panel.

**QA-06.9 Bulk group apply and remove**
- **Steps:** Select 4 tiles; `fill '#bulk-group-key-input' qa-bulk-grp`; click `button=Apply group`; check the gallery order; then `button=Remove from group` (or `Remove from groups`).
- **Expected:** Apply assigns orders 1..N and restarts the query (members become adjacent); removal clears key and order; conflicting groups show a warning and blank key ([groups](../../subsystems/search-tags-and-media-groups.md#favorites-and-media-groups)).
- **Oracle:** DB `media_group_key` and order per asset; adjacent tiles in a screenshot with backplates.

**QA-06.10 Group on a range: order by gallery, no order panel, selection cleared**
- **Steps:** Ctrl+A or a Shift range over more than 10,000 items (a very large write; only this suite does it, then you must restore with Remove group). `fill '#bulk-group-key-input' qa-range`, `Apply group`. Inspect the panel and `[data-testid="bulk-open-order-modal"]`.
- **Expected:** Orders 1..N follow gallery order (then IDs selected outside the range). The `Sort in larger view` button is not available for a range selection. After the write the session changes, so the range is cleared with `[data-testid="bulk-selection-cleared"]` (`Selection cleared`); the next write on the old spec is refused as `stale` ([refresh table](../../subsystems/library-query-and-gallery.md#refresh-and-invalidation-paths)).
- **Oracle:** `select min(media_group_order),max(media_group_order),count(*) from assets where media_group_key='qa-range'` matches 1..N; order follows the gallery `order by`; `inspect '[data-testid="bulk-selection-cleared"]'`. Clean up: select the group (`gN:qa-range` in the search), Ctrl+A, `Remove from group`.

**QA-06.11 Selection cleared after a result change and stale write**
- **Steps:** Select a range (over 10,000 items), then change the result (`ctl select <id> '#gallery-media-kind' value=image`, or add a tag that changes a tag filter).
- **Expected:** `Selection cleared` appears with the explanation text; a new session ID clears ranges. A write attempted on a stale spec writes nothing, and the library refreshes.
- **Oracle:** `inspect '[data-testid="bulk-selection-cleared"]'`; DB counts unchanged; `process.log` `stale` line.

**QA-06.12 Group order modal: drag and keyboard**
- **Steps:** Select 5 tiles, `fill '#bulk-group-key-input' qa-order`, `ctl click <id> '[data-testid="bulk-open-order-modal"]'`. Drag tile 1 to position 4: `ctl drag <id> '[data-testid^="bulk-order-tile-"]' ...`. Focus a handle (`[data-testid^="bulk-order-handle-"]`) and press ArrowRight/ArrowDown. `button=Save order`. Reopen, reorder and close with `Cancel`, `Escape` and by clicking the backdrop.
- **Expected:** Drag shows `[data-testid="bulk-order-insertion-line"]`; keyboard moves announce the new position; Save order writes 1..N; Cancel, Esc and the backdrop discard edits; partial failure shows the retry message ([group order](../../subsystems/search-tags-and-media-groups.md#favorites-and-media-groups)).
- **Oracle:** DB order values after Save; unchanged after Cancel; thumbnails in the modal load via `thumb://`.

**QA-06.13 `Updated X of Y`**
- **Steps:** Select several assets that include some that were removed from disk by `qa_fs.py rm` earlier is S2 territory; here only record whether `Updated X of Y` ever appears. If not provokable, record the gap.
- **Expected:** `Updated X of Y selected items.` appears only for ID-based writes with partial results.
- **Oracle:** `inspect '[data-testid="bulk-action-panel"]'`; gap note otherwise.

### QA-07 Favorites and media groups end to end

Destructive: no. Session: shared S1 session

**QA-07.1 Favorites with the filter on**
- **Steps:** In the lightbox favorite two assets. Close, `ctl click <id> 'button[aria-label="Show favorites only"]'`, `wait idle`. Open one favorite and remove it (`Remove from favorites`).
- **Expected:** Only the favorites show; removing a favorite while the filter is on refreshes and the item disappears; the heart is `aria-pressed="true"`.
- **Oracle:** Footer count equals `select count(*) from assets where is_favorite=1`.

**QA-07.2 MediaGroupSetter: key, copy, UUID**
- **Steps:** Open an asset. `fill '#lightbox-media-group-key-input' qa-grp-a`; `fill '#lightbox-media-group-order-input' 2`; click `button=Apply`. Click `button[aria-label="Generate media group key"]` on another asset and read the key; click `button[aria-label="Copy group name"]`.
- **Expected:** The key is trimmed (blank becomes null). Generate fills a UUID. Copy sets the title `Group name copied` (clipboard content cannot be read under Xvfb; the title is the oracle). The tile gets the `Part of a media group` label and a backplate.
- **Oracle:** DB key and order; `inspect` of the key input matches the UUID pattern `^[0-9a-f-]{36}$`.

**QA-07.3 Order validation**
- **Steps:** In `#lightbox-media-group-order-input` try `0`, `-1`, `1.5`, `1e3`, ` 3`, `abc`, `007`, `99999999999999999999`, empty.
- **Expected:** Only positive safe integers are accepted (leading zeros allowed); invalid values disable Apply and nothing is written; empty means null ([groups](../../subsystems/search-tags-and-media-groups.md#favorites-and-media-groups)).
- **Oracle:** `inspect 'button=Apply'` state (disabled) and the DB row unchanged.

**QA-07.4 `gN:` search and ordering**
- **Steps:** Give three assets key `QA-Grp-B` and orders 3, 1, 2. Search `gN:qa-grp-b`, then `gN:QA-GRP-B`.
- **Expected:** Both return the three assets, case-insensitive; the group shows in order 1, 2, 3; in the full gallery the group is adjacent, with a backplate that joins consecutive cells in one row only.
- **Oracle:** Tile order via `inspect 'button[data-asset-index]'` file names vs DB order; screenshot of the backplate (light and dark in QA-08).

**QA-07.5 Clearing a group and group placement**
- **Steps:** Blank key and order for one member (`Apply` with both empty); check the gallery position of the group when its newest member changes mtime.
- **Expected:** Blank Apply clears key and order; the group sorts by its newest matching member's month ([gallery ordering](../../subsystems/database.md#gallery-and-media-group-ordering)).
- **Oracle:** DB null key/order; search `gN:qa-grp-b` returns two assets.

### QA-08 Non-destructive settings

Destructive: no. Session: shared S1 session

**QA-08.1 Section anchors and navigation**
- **Steps:** `ctl click <id> '#open-settings-button'`, click each link of `nav[aria-label="Settings sections"]` (`a[href="#settings-scan"]`, `#settings-appearance`, `#settings-language`, `#settings-import-export`, `#settings-duplicates`, `#settings-danger`, `#settings-about`). `ctl keys <id> Escape` returns to the gallery.
- **Expected:** Each link scrolls to its section; Back and Escape return; section loaders (`*-section-loader`) disappear.
- **Oracle:** `inspect '#settings-about'`, screenshot per section. Do not click anything in `#settings-danger` or Import/Export in this suite.

**QA-08.2 Theme light and dark**
- **Steps:** In `fieldset[aria-label="Select theme"]` choose Light and Dark; screenshot the gallery, a lightbox, the bulk panel and a group backplate in each.
- **Expected:** Contrast is readable in both; the choice persists after `ctl refresh <id>` (`media-tagger.theme`).
- **Oracle:** Screenshots; `inspect html` theme attribute.

**QA-08.3 Eleven languages**
- **Steps:** For each of the 11 codes in `src/i18n/locales/*.json` (list the file names): `ctl select <id> '#settings-language-select' value=<code>`, screenshot Settings and the gallery, open the lightbox and the delete confirmation (read the word in `#lightbox-delete-confirm-input`'s label, then Escape), open the timeline and read `aria-valuetext`. Finish with `value=en`.
- **Expected:** No raw keys (strings such as `settings.scan.rescanAll`), no `undefined`, no `{{placeholder}}`; no text overflow or clipped buttons; the delete confirmation word is localized; month names in the timeline are in the language; `Intl` number format in the footer follows the language ([localization](../../development/localization.md)).
- **Oracle:** `ctl inspect <id>` text searched for `.` key patterns and `{{`; screenshots reviewed per language; overflow reported with the language code.

**QA-08.4 Render all thumbnails, Stop, Retry failed**
- **Steps:** Settings -> `button=Render thumbnails for all indexed assets`; while running `ctl inspect <id> '#settings-scan'` (progress `Rendering thumbnails n/m`) and `ctl click <id> 'button=Stop thumbnail render'`. Then `button=Retry failed thumbnails only`.
- **Expected:** Stop appears only while a bulk render runs; it ends with `Stop requested...` and a `Cancelled at n/m` summary; Retry failed finishes with `Failed-thumbnail retry finished` and a ready/failed summary ([bulk render](../../subsystems/settings-operations.md#thumbnail-bulk-actions-and-cancellation)).
- **Oracle:** Summary text; `sqlite3 -readonly` count of thumbnails; `process.log` perf lines; the failed count equals the unreadable files of the copy (if any).

**QA-08.5 Operation lock**
- **Steps:** Start Rescan all or Render all, then immediately `inspect` the other buttons (`disabled`). Go Back to the gallery and use it (scroll, lightbox), then return to Settings.
- **Expected:** Only one operation runs; the other buttons are disabled; the operation continues after leaving Settings and its progress is shown again on return ([operation runner](../../subsystems/settings-operations.md#exclusive-operation-runner)).
- **Oracle:** `inspect 'button=Rescan all'` disabled; progress text after returning.

**QA-08.6 ffmpeg status**
- **Steps:** `ctl inspect <id> '[data-testid="video-tool-status"]'`.
- **Expected:** `ffmpeg: available` and `ffprobe: available` (en) when installed; otherwise `not available` ([thumbnail tools](../../subsystems/thumbnails.md#ffmpeg-and-ffprobe-discovery)).
- **Oracle:** The text; compare with `which ffmpeg ffprobe` on the host.

**QA-08.7 About and link error handling**
- **Steps:** `ctl inspect <id> '[data-testid="about-version"]'`. Click `button*=Source code on GitHub` and `button*=Contact on GitHub` (or the anchors in `#settings-about`). Under Xvfb the browser may not open.
- **Expected:** The version matches the build; when the browser cannot open, an alert `Could not open the browser. Copy the address and open it manually.` appears with a `Link address` field (`[role="alert"] input`) holding the URL ([About](../../subsystems/settings-operations.md#about-legal-texts-and-external-links)). License and privacy texts are shown.
- **Oracle:** `inspect` of the alert and field; the process is still alive (`ctl doctor <id>` is `ok`).

**QA-08.8 Startup scan toggle**
- **Steps:** Toggle the `Scan on app startup` checkbox in `#settings-scan` off and on.
- **Expected:** `Saved. Applies on the next app launch.` appears; the value is stored per root.
- **Oracle:** The status text; `select path,auto_scan_on_startup from scan_roots`. Restore the original state.

### QA-09 Mixed 30-45 minute soak

Destructive: no. Session: own perf session (`VITE_MEDIATAGGER_PERF=1`)

**QA-09.1 Set up the perf session**
- **Steps:** Start the session built with `VITE_MEDIATAGGER_PERF=1`; `ctl doctor <id>`; `qa_session.py --run <run> mark --block S1 --suite QA-09 begin`; note the baseline RSS and CPU from the first row of `qa/sessions/*-samples.csv`.
- **Expected:** Perf lines appear in `process.log`; the sampler writes a row every 2 s.
- **Oracle:** `process.log` perf entries; `samples.csv` rows.

**QA-09.2 Mixed action loop (30-45 min)**
- **Steps:** Repeat in rounds of about 3 minutes: scroll to the end and back (`End`, `Home`, wheel 3000/-3000), scrubber jumps, tile size sweep 140 -> 340 -> 188, media-kind switch All/Image/Video, search (tag, `-tag`, `tags:0`, Clear), open the lightbox and walk 50 images forward/back with zoom, tag/favorite writes, bulk mode with a rectangle and Ctrl+A then Esc, tag list open/include/Apply/Clear, Settings and Back, language switch to `pl` and back, a Render all started and stopped. Record the round number in the suite notes.
- **Expected:** No growth of unreleased state: RSS stays within the plateau reached after the first 10 minutes (no steady upward slope); CPU is near idle between actions; the UI answers within a few seconds throughout.
- **Oracle:** Slope of RSS and CPU columns in `qa/sessions/*-samples.csv` (report first, 10-minute, last and max RSS in kB); `ctl doctor <id>` is `ok` at each round end.

**QA-09.3 Console warnings and process log**
- **Steps:** Every 10 minutes and at the end `ctl logs <id>`; grep `console.jsonl` for `warn`, `error`, `unhandledrejection` and `process.log` for `panic`, `503`, `WARN`.
- **Expected:** No errors or panics; the warning set does not grow in time. The perf lines show no frame or query timings degrading over time.
- **Oracle:** Count of warnings per 10-minute window with timestamps; list every distinct message.

**QA-09.4 Refresh and state return**
- **Steps:** At the end `ctl refresh <id>`, `ctl wait <id> 'button[data-asset-index="0"]'`, then `ctl wait <id> idle 60000`, `ctl doctor <id>`. Compare tile size (188), filters (cleared), theme and language with the state before the refresh.
- **Expected:** The app returns to a working gallery; tile size resets to 188; persisted values (theme, language) survive; the console gap is recorded; memory returns to near baseline after the refresh.
- **Oracle:** `inspect '#tile-size'`, theme/language values, last `samples.csv` rows; `qa_session.py --run <run> mark --block S1 --suite QA-09 end`. Do not close the window unless the brief says this is the last step.

## Block S2: mutations and data safety

The block copies `source/` and the first photo folder of `$AUDIT_DATASET` (`zdjecia_*:first`) into `<scratch>/media/dataset/`. Edge files are created with `qa_fs.py write-edge` below `<scratch>/media/edge/` (at most 50 per session; kinds `zero-byte`, `corrupt-jpg`, `wrong-ext`, `png`, `symlink-inside`). The tester role is `qa_tester_safety` on Fable. Every suite is destructive: the clerk runs the dataset guard after each one, and the tester never touches paths outside `<scratch>/media`. All file changes go through `python3 .claude/skills/audit/scripts/qa_fs.py --run <run> <copy|touch|rm|mv|chmod|write-edge> ...`, with paths relative to the session media directory.


### QA-10 Delete
Destructive: yes.

**QA-10.1 Confirm word, English**
- **Steps:** pick a file in `edge/` or a copied photo and record `ls -la`, its `assets` row (`SELECT id,path,thumb_path FROM assets WHERE file_name='<f>'`) and the thumbnail file. Open it, `click 'button[aria-label="Delete media"]'`. Fill `#lightbox-delete-confirm-input` with `x`, then with `yes` (lowercase), check the Confirm button state with `inspect`.
- **Expected:** Confirm stays disabled until the localized word matches (the comparison trims, NFC-normalizes and lowercases both sides with `toLocaleLowerCase` in the active language, so `yes`/`YES` match; accents stay significant); the word is "Yes" (`docs/subsystems/lightbox.md#toolbar-sidebar-confirmation-and-clipboard`).
- **Oracle:** `inspect '[data-testid="lightbox-delete-confirm-dialog"]'` (disabled attribute), screenshot.

**QA-10.2 Confirm word, Polish**
- **Steps:** `select '#settings-language-select' value=pl` in Settings, back, open another file, start delete, type `Tak`, then `Yes`.
- **Expected:** only "Tak" (and case variants) enables Confirm; the dialog is Polish ("Czy na pewno chcesz usunac to medium?"). Switch back to `en` at the end (`docs/subsystems/lightbox.md`).
- **Oracle:** `inspect` of the dialog text, screenshot, no console errors.

**QA-10.3 Cancel and Esc leave the file intact**
- **Steps:** open delete, type the word, click Cancel; reopen, press `keys Escape` with focus in the input; reopen and press Escape with the lightbox focused on the rest.
- **Expected:** Cancel and Esc dismiss only the inline confirmation (the lightbox stays open), clear the input and restore focus to the delete button; nothing is deleted (`docs/subsystems/lightbox.md#toolbar-sidebar-confirmation-and-clipboard`, Esc queue).
- **Oracle:** `ls`/`stat` unchanged, `DB` row count unchanged, `inspect '[role="dialog"]'`.

**QA-10.4 Successful delete removes file, row, thumbnail**
- **Steps:** record the path, `thumb_path` and tag/favorite rows of the asset; confirm the delete.
- **Expected:** the lightbox closes, the tile disappears, the file is gone from the copy, the `assets` row and `asset_tags` rows are gone, orphaned tags vanish, the thumbnail file is gone, the library count in `gallery-end-state` drops by one, no `.mediatagger-*.pending` file is left (`docs/subsystems/data-safety-and-portability.md#operation-safety-matrix`).
- **Oracle:** `ls -la` (file and `.mediatagger-*.pending` in its directory), `DB 'SELECT count(*) FROM assets WHERE path=...'`, `DB 'SELECT count(*) FROM pending_file_operations'` (expect 0), `ls <app-data>/thumbs`, `inspect '[data-testid="gallery-end-state"]'`.

**QA-10.5 File deleted externally: `missing` alert**
- **Steps:** `qa_fs.py rm <photo>/<file>` for an indexed file (no rescan). Open it in the lightbox (it may show a media error), delete, confirm. Doctor now reports `alert-open`: `ctl alert text`, then `ctl alert accept`.
- **Expected:** a `window.alert` with "The source file was already missing. Its stale library entry was removed."; after accept the stale row and tile are gone (`docs/subsystems/data-safety-and-portability.md#operation-safety-matrix`, `missing`).
- **Oracle:** `alert text` output, `console.jsonl` alert entry, `DB` row gone, `doctor` returns `ok` after accept.

**QA-10.6 `cleanup_pending` attempt**
- **Steps:** make final cleanup likely to fail: `qa_fs.py chmod 555 <dir>` on the file's directory before deleting a different indexed file (the staging rename needs write access, so the delete is expected to fail pre-commit; also try `chmod 555` only after opening the lightbox). Record the outcome of each variant. Restore with `qa_fs.py chmod 755 <dir>`.
- **Expected:** a pre-commit failure shows the inline error "The file could not be deleted. Its library metadata was preserved." and keeps the confirmation open; `cleanup_pending` (alert with a recovery path) only if the commit succeeded. If `cleanup_pending` cannot be produced from the UI, record it as a gap (`docs/subsystems/data-safety-and-portability.md#file-mutation-commit-and-recovery`).
- **Oracle:** `inspect` the inline alert, `alert text` if raised, `DB` row and `pending_file_operations`, `ls -la` for a `.mediatagger-*.pending` leftover.

**QA-10.7 Rapid consecutive deletes**
- **Steps:** open an asset, delete it, and as soon as the lightbox closes open the next tile and delete again; repeat 8 times, then also double-click Confirm via two `click` calls in quick succession.
- **Expected:** exactly one delete per asset, no duplicate-submit, no stuck lock, the grid stays consistent (`docs/subsystems/lightbox.md`, duplicate-submit guard).
- **Oracle:** `DB 'SELECT count(*) FROM assets'` minus 8, `ls` counts, `console.jsonl`, `doctor`.

**QA-10.8 Delete under the favorites filter**
- **Steps:** favorite 3 assets, enable the favorites filter (`click 'button[aria-label="Show favorites only"]'`), open one favorite, delete it.
- **Expected:** it disappears from the filtered grid, the total drops by one, the remaining favorites stay; navigation after the delete does not land on a missing item (`docs/subsystems/library-query-and-gallery.md`).
- **Oracle:** `inspect '[data-testid="gallery-end-state"]'`, `DB 'SELECT count(*) FROM assets WHERE is_favorite=1'`.

**QA-10.9 Delete while a metadata write is queued**
- **Steps:** in the lightbox, run `fill '#lightbox-tag-draft-input' q1,q2` style sequences (`fill` then `keys Enter`) and immediately click Delete and Confirm; also toggle the heart right before confirming.
- **Expected:** the per-asset FIFO write queue is rejected or drained for the deleted asset without an unhandled rejection; the file, row and tags are gone and no orphan `asset_tags` rows remain (`docs/subsystems/search-tags-and-media-groups.md`).
- **Oracle:** `DB 'SELECT count(*) FROM asset_tags WHERE asset_id NOT IN (SELECT id FROM assets)'` (0), `console.jsonl` (no unhandled rejection), screenshot.

### QA-11 Duplicate manager
Destructive: yes.

**QA-11.1 Create case-only name duplicates and scan**
- **Steps:** `qa_fs.py mv dataset/<photo>/<a>.jpg dataset/<photo>/Dup.JPG`, `qa_fs.py mv dataset/<photo>/<b>.jpg dataset/<photo>/dup.jpg` (and one more pair in `source/`). Settings, Scan, Rescan all, then Duplicates `click 'button=Start'`.
- **Expected:** one group keyed `dup.jpg` containing both spellings (identity is the lowercase basename); the resolver dialog `#duplicate-resolver-heading` shows "Found groups: N" (`docs/subsystems/data-safety-and-portability.md#duplicate-identity`).
- **Oracle:** `DB 'SELECT file_name,path FROM assets WHERE file_name_key="dup.jpg"'`, `inspect '#duplicate-resolver-heading'`, screenshot.

**QA-11.2 Rename validation**
- **Steps:** per item, `fill` the rename input (`input[data-asset-id="<id>"]`) with: empty, `a/b.jpg`, `..`, `../x.jpg`, `a\b.jpg`, `con.jpg`, `name.jpg.` , a name equal to an existing file, an existing name differing in case; click `Queue rename` each time (it is disabled for a blank draft).
- **Expected:** invalid names are rejected by the UI validation or by Save all; nothing is written; no group may keep two items with the same lowercase name ("queued changes still leave duplicate file names") (`docs/subsystems/settings-operations.md#duplicate-resolver`).
- **Oracle:** `inspect` the resolver (pending text, validation message), `ls` on the folder unchanged.

**QA-11.3 Generate UUID rename**
- **Steps:** click `button=Generate UUID` on both items of a group, keep the extension, `Queue rename` on both, `Save all`.
- **Expected:** the draft keeps the extension; a pure rename batch applies without confirmation; result "Applied N changes"; the group disappears (`docs/subsystems/settings-operations.md#duplicate-resolver`).
- **Oracle:** `ls -la` shows UUID names, `DB 'SELECT path FROM assets WHERE id IN (...)'`, `DB 'SELECT value FROM library_metadata'` (the revision) increased by one, thumbnails still shown.

**QA-11.4 Queue delete, Unmark, Clear, Discard**
- **Steps:** `Queue delete` on one item (the rename input becomes disabled), `Unmark delete`, `Queue rename`, `Clear action`, queue again, then `Discard changes`.
- **Expected:** an asset has at most one staged action; delete replaces a staged rename; Unmark and Clear remove it; Discard clears everything (`docs/subsystems/settings-operations.md#duplicate-resolver`).
- **Oracle:** `inspect` the "Pending: ..." line of the card and "Queued changes: N".

**QA-11.5 Save all with delete: second dialog**
- **Steps:** queue one rename and one delete in a group, `Save all`. In the nested dialog (`#duplicate-delete-confirm-heading`) first press Escape, then `Cancel`, then `Save all` again and `Confirm`.
- **Expected:** a batch containing a delete always opens the second confirmation ("This will permanently delete N files from disk"); Cancel/Esc applies nothing and returns to the resolver; Confirm applies rename and delete together (`docs/subsystems/settings-operations.md#confirmation-ordering-and-close-behavior`).
- **Oracle:** `ls -la` before and after, `DB` row for the deleted asset gone, `pending_file_operations` empty, dataset guard by the clerk.

**QA-11.6 Rollback when the directory is not writable**
- **Steps:** queue a rename in `<photo>` and a rename in `source/<dir>`; `qa_fs.py chmod 555 dataset/source/<dir>`; `Save all`. Restore with `chmod 755` afterwards.
- **Expected:** the whole batch is rolled back ("all file changes were rolled back. No library metadata was changed") or reports recovery required with paths; no file in `<photo>` is renamed (`docs/subsystems/data-safety-and-portability.md#file-mutation-commit-and-recovery`).
- **Oracle:** `ls` shows original names, `DB` paths unchanged, `ls -la` finds no `.mediatagger-*.pending`, screenshot of the result message.

**QA-11.7 Stale snapshot**
- **Steps:** open the resolver, stage a rename, rename a file in the copy with `qa_fs.py mv` (no rescan), `Save all`.
- **Expected:** the stale snapshot is rejected before any file is moved; use `Rescan` in the dialog to refresh (`docs/subsystems/data-safety-and-portability.md#duplicate-identity`).
- **Oracle:** error message via `inspect`, `ls` unchanged.

**QA-11.8 Dialog keyboard and focus**
- **Steps:** with the resolver open press Tab repeatedly (focus must wrap inside); press Escape; reopen via Start; open the nested delete dialog and press Escape.
- **Expected:** Tab wraps within the top modal; Escape closes only the top layer; focus returns to the trigger (`docs/frontend/architecture-and-ui-conventions.md#modal-and-overlay-conventions`).
- **Oracle:** `inspect` of `document.activeElement` is not available, so use screenshots plus `keys Tab` loops and `inspect '[role="dialog"]'` (there is no active-element query; the focus ring needs a native click first).

### QA-12 External changes and rescan
Destructive: yes.

**QA-12.1 External delete**
- **Steps:** `qa_fs.py rm dataset/<photo>/<f>`, Settings, `button=Rescan all` (or Rescan on the root), back to the gallery.
- **Expected:** the asset is pruned (row and tile gone, count drops); its thumbnail file may stay (scan pruning does not delete thumbnails) (`docs/subsystems/scanning-and-indexing.md#known-limitations`).
- **Oracle:** `DB 'SELECT count(*) FROM assets WHERE path=...'`, `ls <app-data>/thumbs | wc -l`, `inspect '[data-testid="gallery-end-state"]'`.

**QA-12.2 External rename**
- **Steps:** `qa_fs.py mv dataset/<photo>/<f>.jpg dataset/<photo>/renamed.jpg`, rescan. Before that record the asset's tags and favorite.
- **Expected:** a new asset with the new name appears and the old row is pruned; tags/favorite of the old row are not carried over (path is identity) (`docs/subsystems/scanning-and-indexing.md#path-identity-and-discovery`).
- **Oracle:** `DB 'SELECT id,file_name FROM assets WHERE file_name IN (...)'`, `asset_tags` count for the new id.

**QA-12.3 Content change and thumbnail identity**
- **Steps:** record `thumb_path`, `size_bytes`, `modified_at` and the tile `img` src (`inspect 'button[data-asset-id="<id>"] img'`, the `?v=` value). Replace content with `qa_fs.py write-edge png edge/x.png` plus `mv` over the file (same name), and `qa_fs.py touch ... --days-ago 1`. Rescan.
- **Expected:** the row is re-fingerprinted; `thumb_path` is cleared and re-rendered, and the thumbnail URL version (`?v=`) changes. Record whether a same-second change is detected (`docs/subsystems/scanning-and-indexing.md#fingerprints-and-extracted-metadata`).
- **Oracle:** `DB 'SELECT size_bytes,modified_at,thumb_path FROM assets WHERE path=...'`, tile `img` src before/after, screenshot.

**QA-12.4 New files and nested directories**
- **Steps:** `qa_fs.py write-edge png edge/new1.png` ... five files, plus `mv` of one photo into a new subfolder; rescan.
- **Expected:** new files are indexed and appear in the grid; progress is shown ("Scanning N/M"); the counter in `gallery-end-state` grows (`docs/subsystems/scanning-and-indexing.md#progress-and-results`).
- **Oracle:** `DB 'SELECT count(*) FROM assets'`, `inspect '[data-testid="gallery-end-state"]'`.

**QA-12.5 Unreadable file (`chmod 000`)**
- **Steps:** `qa_fs.py chmod 000 dataset/<photo>/<f>.jpg`, then `chmod 000` on a whole subdirectory in `source/`; rescan; open the file's tile if still present.
- **Expected:** the scan completes (traversal errors make it partial, no crash); an unreadable directory contributes no assets; a thumbnail failure is recorded for an unreadable file, and the tile shows a failed state without hanging the loader. Restore with `chmod 644`/`755`.
- **Oracle:** `DB 'SELECT count(*) FROM thumbnail_failures'`, `process.log`, `console.jsonl`, `wait '[data-testid="tile-thumb-loader"]' hidden 30000`.

**QA-12.6 Corrupt, zero-byte and mislabeled files**
- **Steps:** `write-edge corrupt-jpg edge/bad.jpg`, `write-edge zero-byte edge/zero.jpg`, `write-edge wrong-ext edge/notanimage.jpg`; rescan; open each tile.
- **Expected:** kind detection is by extension only, so they are indexed; dimensions are absent; thumbnails fail without breaking the grid; the lightbox shows "Could not display image" (`[data-testid="lightbox-media-error"]`) and remains closable (`docs/subsystems/scanning-and-indexing.md#known-limitations`).
- **Oracle:** `DB 'SELECT file_name,width,height FROM assets WHERE path LIKE "%edge/%"'`, `thumbnail_failures`, screenshot, console.

**QA-12.7 Unicode, spaces and long names**
- **Steps:** `qa_fs.py mv` a photo to names such as `zażółć gęślą jaźń.jpg`, `a   b  .jpg`, a 200-character name, a name with `%` and `_`; rescan; search by tag and open the lightbox.
- **Expected:** all are indexed with exact names; tiles, alt text and the lightbox title show them without clipping that breaks layout; thumbnails load (`docs/subsystems/scanning-and-indexing.md#path-identity-and-discovery`).
- **Oracle:** `DB 'SELECT file_name FROM assets WHERE file_name LIKE "%a%"'`, screenshot, `thumb://` requests without 404 in `process.log`.

**QA-12.8 Symlinks are not followed**
- **Steps:** `qa_fs.py write-edge symlink-inside edge/link.jpg`; also create a symlinked directory the same way if the tool allows; rescan.
- **Expected:** symlinked files and directories are absent from the library (`follow_links(false)`), the real file is indexed once (`docs/subsystems/scanning-and-indexing.md#path-identity-and-discovery`).
- **Oracle:** `ls -la edge/`, `DB 'SELECT count(*) FROM assets WHERE path LIKE "%link.jpg"'` equals 0.

**QA-12.9 Rescan idempotence and concurrency**
- **Steps:** `Rescan all` twice in a row, then switch to the gallery during a scan and scroll; press Rescan on a single root while another operation is running.
- **Expected:** a second identical scan changes nothing (no thumbnail churn); other settings buttons are disabled during the exclusive operation; the gallery stays responsive (`docs/subsystems/settings-operations.md#exclusive-operation-runner`).
- **Oracle:** `DB count`, `process.log` scan lines, `inspect` of disabled buttons.

### QA-13 Remove root and Clear library
Destructive: yes.

**QA-13.1 Remove root: cancel and confirm**
- **Steps:** Settings, `Remove` on the root. In the dialog (`#remove-scan-root-confirm-heading`) press Escape, then open again and click `No`/Cancel equivalent, then open again and confirm. Run `ctl doctor` during the operation.
- **Expected:** nothing is removed on cancel; on confirm the root, its assets, thumbnails and orphan tags are removed; original files stay on disk; the gallery shows the empty state "No folders attached" (`docs/subsystems/settings-operations.md#scan-roots-and-rescanning`, `docs/subsystems/data-safety-and-portability.md#operation-safety-matrix`).
- **Oracle:** `DB 'SELECT count(*) FROM scan_roots'`, `assets`, `tags`; `ls -la <scratch>/media` files unchanged; `ls <app-data>/thumbs`.

**QA-13.2 Maintenance lock behavior**
- **Steps:** during Remove root and Clear library poll `ctl doctor` and `ctl wait idle 90000`.
- **Expected:** locks wait without a limit, so doctor may report `busy-maintenance` (a probe timed out) while observation commands (`inspect`, `screenshot`, `logs`) work and actions are refused; it returns to `ok` when done. Record the duration (`docs/subsystems/settings-operations.md#exclusive-operation-runner`).
- **Oracle:** doctor output with timestamps from `actions.jsonl`, `process.log` progress lines.

**QA-13.3 Empty root list state**
- **Steps:** after removal inspect Settings (`No scan paths`), the gallery empty state, `Rescan all` (disabled), and the search field.
- **Expected:** consistent empty states, no errors; "Add first folder" works from the gallery (`docs/subsystems/library-query-and-gallery.md`).
- **Oracle:** `inspect`, screenshot, `console.jsonl`.

**QA-13.4 Re-add the root through the native picker**
- **Steps:** `click 'button=Choose folder'` (or "Add first folder"); a native dialog opens: `native key Control+l`, `native type <scratch>/media` (the path doctor prints), `native key Return`. Wait for the scan.
- **Expected:** the root returns, a scan starts and the library repopulates; the original files are still all there. Taint check: the doctor after `native` must be `ok` (`docs/subsystems/scanning-and-indexing.md#root-lifecycle`).
- **Oracle:** `screenshot` of the dialog first, `DB 'SELECT path FROM scan_roots'`, `assets` count equals the file count (`find <scratch>/media -type f | wc -l` minus unsupported files).

**QA-13.5 Clear library: No, then Yes**
- **Steps:** Settings, Danger zone, `button=Remove all thumbnails, indexed assets and scan paths`; in the dialog click `No`; again and press Escape; again and click `YES`. Poll doctor while it runs.
- **Expected:** nothing happens on No/Esc; YES removes assets, tags, roots and thumbnails (including favorites and groups); original files remain; the maintenance barrier drains queued metadata writes; roots reload empty (`docs/subsystems/settings-operations.md#clear-library`).
- **Oracle:** `DB 'SELECT (SELECT count(*) FROM assets),(SELECT count(*) FROM tags),(SELECT count(*) FROM scan_roots)'` all zero, `ls <app-data>/thumbs`, `ls` copy unchanged.

**QA-13.6 Clear library with a queued metadata write and open lightbox**
- **Steps:** open a lightbox, queue a tag, go back to Settings and clear.
- **Expected:** the queued write drains before the clear; no error toast, no resurrected tag after re-adding the root (`docs/subsystems/data-safety-and-portability.md#operation-safety-matrix`).
- **Oracle:** `DB tags` after re-adding the root is empty, `console.jsonl`.

**QA-13.7 Recovery to a working state**
- **Steps:** re-add the root (QA-13.4), wait for the full scan and thumbnails (`wait '[data-testid="tile-thumb-loader"]' hidden 60000`).
- **Expected:** the app is fully usable again; this state is the precondition for QA-14.
- **Oracle:** `DB` asset count, screenshot of the grid.

### QA-14 CSV and backup
Destructive: yes.

All files are written inside `<scratch>/media/` through the native save dialog (`native key Control+l`, `native type <scratch>/media/<name>`, `native key Return`); never outside the copy. Before long backup steps run `python3 .claude/skills/audit/scripts/qa_session.py --run <run> maintenance --minutes 60`.

**QA-14.1 CSV export**
- **Steps:** tag/favorite/group a few assets, Settings, `button=Export tags CSV`, save as `<scratch>/media/export/tags.csv` (the directory may not exist: see QA-14.8).
- **Expected:** a header row with five standard columns and one row per asset; summary "CSV exported. Rows: N" (`docs/subsystems/data-safety-and-portability.md#csv-metadata-transfer`).
- **Oracle:** `head`, `wc -l` on the file, the summary text via `inspect '[role="status"]'`.

**QA-14.2 CSV import: tags merge**
- **Steps:** change tags in the DB through the UI (add `zz-extra`), then import the exported CSV (`button=Import tags CSV`, no confirmation).
- **Expected:** the CSV tags are merged additively; `zz-extra` stays; counters match the rules (`rows_read`, `rows_applied`, `assets_matched`, `assets_updated`) (`docs/subsystems/data-safety-and-portability.md#merge-counters-and-transaction-failure`).
- **Oracle:** `DB 'SELECT t.name FROM asset_tags a JOIN tags t ON t.id=a.tag_id WHERE a.asset_id=<id>'`, the summary text.

**QA-14.3 CSV import: favorites overwrite and group clear**
- **Steps:** the tester cannot edit files, so export a CSV, change state in the UI (favorite off, group set), then re-import that CSV; for the group clear, import an export taken while the group was empty.
- **Expected:** a non-blank favorite overwrites the current value; a blank favorite keeps it; a blank group key clears the group; case-only name duplicates all get the update (basename match, case-insensitive) (`docs/subsystems/data-safety-and-portability.md#import-headers-parsing-and-matching`).
- **Oracle:** `DB 'SELECT is_favorite,media_group_key,media_group_order FROM assets WHERE id=<id>'`.

**QA-14.4 CSV invalid input**
- **Steps:** import a non-CSV file from the copy (a jpg or a `zero-byte` edge file).
- **Expected:** the whole document is rejected with "CSV import error" and no DB change (`docs/subsystems/data-safety-and-portability.md#import-headers-parsing-and-matching`).
- **Oracle:** `inspect` error text, `DB` favorites/tags counts unchanged.

**QA-14.5 Backup export, Clear library, import**
- **Steps:** export `button=Export DB + thumbnails` to `<scratch>/media/backup/bundle.zip`, timing it (`actions.jsonl` timestamps); poll doctor during the export. Then Clear library (QA-13.5), then `button=Import DB + thumbnails`, choose the bundle, confirm the overwrite dialog (`#import-db-overwrite-confirm-heading`), keep polling doctor.
- **Expected:** doctor may report `busy-maintenance` during export/import (observe-only); after import the roots, assets, tags, favorites, groups and thumbnails are back; the summary shows "Files: N, thumbnails: M"; cancelling the overwrite dialog changes nothing (`docs/subsystems/data-safety-and-portability.md#bundle-import-and-replacement-sequence`).
- **Oracle:** `ls -la backup/bundle.zip`, `DB` counts before export and after import (never query while the import runs), `ls <app-data>/thumbs | wc -l`, durations from `actions.jsonl`, `process.log`.

**QA-14.6 Round trip**
- **Steps:** before export snapshot `DB 'SELECT id,path,is_favorite,media_group_key,media_group_order FROM assets ORDER BY id'` and the tag join into text; repeat after import and compare.
- **Expected:** identical tags, favorites and groups, identical IDs; the gallery shows the same thumbnails without regenerating all of them (`docs/subsystems/data-safety-and-portability.md#bundle-import-and-replacement-sequence`).
- **Oracle:** the two query outputs (diff by eye or `sort`/`uniq -u` on saved text), screenshot.

**QA-14.7 Cancelled pickers**
- **Steps:** open each of the four pickers (CSV export/import, DB export/import) and cancel with `native key Escape`.
- **Expected:** "... cancelled." summaries, no files, no state change, buttons re-enabled (`docs/subsystems/settings-operations.md#import-and-export-ui-orchestration`).
- **Oracle:** `inspect '[role="status"]'`, `ls` for stray files.

**QA-14.8 Export path outside allowed roots, and non-existent directories**
- **Steps:** in the save dialog type a path in a directory that does not exist inside the copy (`<scratch>/media/new-dir/a.csv`); then try a path outside the copy only if the dialog would leave it inside `<scratch>` (for example `<scratch>/outside/a.csv`); never type any path outside the scratch root.
- **Expected:** record whether the app creates missing directories, refuses the path or reports an error; the profile allowlist for writes is described in the system overview. Any file that appears must be inside the scratch root. A typed path whose folder does not exist is refused by GTK itself ("The folder contents could not be displayed"), so the app never receives it: doctor reports `unresponsive` while that dialog is open; screenshot it, dismiss it with `native key Escape`, then cancel the picker with `native key Escape` and expect "CSV export cancelled.".
- **Oracle:** `ls -la` of the target directories, `inspect` of the error text, `process.log`.

**QA-14.9 Zip-2 backup compatibility**
- **Steps:** none.
- **Expected:** gap: no backup produced by the zip 2 build exists in the dataset; import of such a bundle is not testable in this run (report it as a gap, not a pass).
- **Oracle:** the gap is listed in the block summary.

## Block S3: media

The block copies `videos/` from `$AUDIT_DATASET` and relies on the controller's own GIF and MP4 fixtures (the E2E GIF and two MP4 files that the session indexes at start). No edge files. The tester role is `qa_tester`. Video is rendered by libmpv on a GTK surface above the WebView: the DOM has only the container `[data-lightbox-video-player]` (with `data-native-paused`, `data-native-time`, `data-native-muted`, `data-native-volume`, `data-native-rate`, `data-native-fullscreen`, `data-native-session` attributes, read with `inspect`); GTK controls have no selectors and are driven with `native click <x> <y>` on coordinates from a screenshot (1440x900 display). Black MP4 pixels under Xvfb are a known limit and not a bug; judge playback from the `data-native-*` attributes and the GTK control state.

### QA-15 Video, GIF and mixed navigation
Destructive: no.

**QA-15.1 Autoplay and loop**
- **Steps:** `wait 'button[data-asset-id]'`, set the media filter `select '#gallery-media-kind' value=video` (options All, Images, GIF, Video), open a short video, `inspect '[data-lightbox-video-player]'` twice 2 s apart, and again after its duration.
- **Expected:** playback starts automatically (`data-native-paused="false"`), `data-native-time` advances and wraps (loop) (`docs/subsystems/lightbox.md#media-presentation`).
- **Oracle:** `data-native-time` samples, screenshot of the GTK controls.

**QA-15.2 Click pauses**
- **Steps:** `click '[data-lightbox-video-player]'`, then again.
- **Expected:** the first click pauses and focuses the container, the second resumes (`docs/subsystems/lightbox.md#fullscreen-and-keyboard-behavior`).
- **Oracle:** `data-native-paused` true/false.

**QA-15.3 Space and Enter**
- **Steps:** with the player focused (after a click and a resume) `keys Space`, `keys Enter`; then move focus elsewhere (click the sidebar) and press them again.
- **Expected:** with focus on the picture, Space/Enter toggle pause; elsewhere they do not toggle the video (`docs/subsystems/lightbox.md#fullscreen-and-keyboard-behavior`).
- **Oracle:** `data-native-paused`.

**QA-15.4 Arrow keys: seek or navigate**
- **Steps:** pause, note `data-native-time`; focus the picture and `keys ArrowRight` (expect +5 s), `keys ArrowLeft` (-5 s) and at the start (clamped to 0); then focus the sidebar or `body` and press `ArrowRight`.
- **Expected:** with player focus the arrows seek by 5 s preserving pause state; without focus they navigate to the next/previous global result with wrap (`docs/subsystems/lightbox.md#fullscreen-and-keyboard-behavior`).
- **Oracle:** `data-native-time` deltas, `inspect '[role="dialog"]'` (aria-label changes on navigation).

**QA-15.5 Native play/pause and seek bar**
- **Steps:** `screenshot`, Read it, find the GTK control bar; `native click <x> <y>` on Play/Pause and on the seek bar at 25 percent, 75 percent.
- **Expected:** the buttons toggle `data-native-paused`; clicking the seek bar moves `data-native-time` accordingly. After each `native` command doctor must be `ok` (otherwise the session is `tainted`, report it).
- **Oracle:** `data-native-*` attributes, screenshot.

**QA-15.6 Mute and volume**
- **Steps:** `native click` Mute twice, then click and `native drag` on the volume slider.
- **Expected:** `data-native-muted` toggles; `data-native-volume` follows the slider (`docs/subsystems/lightbox.md#media-presentation`).
- **Oracle:** `data-native-muted`, `data-native-volume`.

**QA-15.7 Speed popover**
- **Steps:** `native click` the speed button, screenshot, choose 0.5x, 1x, 1.5x, 2x in turn; open it again and press `keys Escape`.
- **Expected:** `data-native-rate` matches the choice; Escape closes only the popover while the lightbox stays open (`docs/subsystems/lightbox.md#fullscreen-and-keyboard-behavior`).
- **Oracle:** `data-native-rate`, `inspect '[role="dialog"]'`.

**QA-15.8 Fullscreen and the Esc queue**
- **Steps:** `keys f`; check `data-native-fullscreen`; `keys Escape` (exits fullscreen, lightbox stays); `native click` the fullscreen GTK button; in fullscreen open the speed popover and press `keys Escape` three times, noting the effect of each; finally `keys Escape` closes the lightbox.
- **Expected:** Esc order: speed menu, then fullscreen, then (confirm delete / drawer) then the lightbox; `F` toggles fullscreen from WebView and GTK controls; playback position survives fullscreen (`docs/subsystems/lightbox.md#fullscreen-and-keyboard-behavior`).
- **Oracle:** `data-native-fullscreen`, `data-native-time`, `inspect '[role="dialog"]'` after each Escape, screenshots.

**QA-15.9 Image fullscreen and zoom on mixed media**
- **Steps:** open an image from the copy, `keys f`, `keys Escape`, `keys +`, `keys 0`.
- **Expected:** image fullscreen targets the lightbox shell and keeps the sidebar; zoom keys do nothing on a video (`docs/subsystems/lightbox.md#image-fit-zoom-and-pan`).
- **Oracle:** `inspect '[data-testid="lightbox-image"]'`, screenshot.

**QA-15.10 Mixed navigation image, GIF, video**
- **Steps:** with the "All" filter open an image and press `keys ArrowRight` repeatedly across an image, the GIF fixture and a video; then back with `ArrowLeft`; read `data-lightbox-kind` each time.
- **Expected:** the kind changes among `image`, `gif` and `video`; native video sessions start and stop cleanly with no leftover GTK surface over an image; wrap-around works (`docs/subsystems/lightbox.md#global-navigation-and-library-synchronization`).
- **Oracle:** `inspect '[role="dialog"]'` (`data-lightbox-kind`), screenshots, `console.jsonl`.

**QA-15.11 GIF behavior**
- **Steps:** open the GIF in the gallery and lightbox.
- **Expected:** the tile has the "GIF" chip and does not animate; the lightbox shows the GIF as an image.
- **Oracle:** screenshot twice, 1 s apart (tile identical), `inspect` the chip text.

**QA-15.12 Duration in info and chips**
- **Steps:** open a video, expand the Information panel (`[data-testid="lightbox-info-panel"]`).
- **Expected:** the Duration row is shown for videos and matches `duration_ms`; images have no duration (`docs/subsystems/scanning-and-indexing.md#fingerprints-and-extracted-metadata`).
- **Oracle:** `inspect '[data-testid="lightbox-info-panel"]'`, `DB 'SELECT file_name,duration_ms FROM assets WHERE kind="video"'`.

**QA-15.13 Thumbnails of long videos**
- **Steps:** in the gallery with the Video filter, `wait '[data-testid="tile-thumb-loader"]' hidden 90000`; compare tiles for the longest videos; Settings, `button=Retry failed thumbnails only` if any failed; check `[data-testid="video-tool-status"]`.
- **Expected:** every video gets a thumbnail within a reasonable time (record seconds for the longest); failures are listed in `thumbnail_failures`, not hidden (`docs/subsystems/thumbnails.md`).
- **Oracle:** `DB 'SELECT count(*) FROM assets WHERE kind="video" AND thumb_path IS NULL'`, `thumbnail_failures`, screenshot.

**QA-15.14 Delete a video while playing**
- **Steps:** during playback open Delete, confirm (copy only).
- **Expected:** the native surface is torn down, the lightbox closes, the file is removed from the copy and DB, no stuck audio surface (`docs/subsystems/lightbox.md#toolbar-sidebar-confirmation-and-clipboard`).
- **Oracle:** `ls`, `DB`, `process.log` (libmpv errors), screenshot.

**QA-15.15 Known limits**
- **Steps:** screenshot a playing MP4.
- **Expected:** the picture may be black on Xvfb; mark as a known limit, not a defect. Native video pixels remain unverified.
- **Oracle:** screenshot, the `data-native-time` evidence that playback advanced.

## Block S4: keyboard, accessibility, window

The block copies the first photo folder of `$AUDIT_DATASET` (`zdjecia_*:first`); there are no edge files. The tester role is `qa_tester`. The suite is non-destructive; the last step is Close window, which ends the session, so it is performed once at the very end (and only if the clerk's brief says so). Xvfb has no window manager: maximize, minimize and minimum-size behavior are gaps.

### QA-16 Keyboard, accessibility, window
Destructive: no.

**QA-16.1 Focus trap and Tab order in every modal**
- **Steps:** for each modal (tag list `[data-testid="tag-list-modal"]` via `button[aria-label="Open tag list"]`; group order `[data-testid="bulk-order-modal"]` via `[data-testid="bulk-open-order-modal"]` in bulk mode with a group selected; remove-root; clear-library; duplicate resolver and its nested delete confirmation; DB import overwrite) press `keys Tab` 15 times and `keys Shift Tab`, taking a screenshot every few presses, then `keys Escape`.
- **Expected:** initial focus is `[data-initial-focus]` or the first tabbable; Tab and Shift+Tab wrap inside the top modal; `#root` is inert; Escape closes the top layer and focus returns to the trigger (`docs/frontend/architecture-and-ui-conventions.md#modal-and-overlay-conventions`).
- **Oracle:** screenshots of the focus ring (focus-visible needs a native click first: `native click` inside the modal), `inspect '[role="dialog"][aria-modal="true"]'`, `inspect '#root'` for `inert`.

**QA-16.2 Dialog names**
- **Steps:** `inspect '[role="dialog"]'` for the lightbox, tag list, group order, remove-root, clear-library, duplicate dialogs.
- **Expected:** every dialog has `aria-modal="true"` and a translated `aria-label` or `aria-labelledby` pointing at an existing heading (`#tag-list-modal-heading`, `#remove-scan-root-confirm-heading`, `#clear-library-confirm-heading`, `#duplicate-resolver-heading`, `#duplicate-delete-confirm-heading`, `#import-db-overwrite-confirm-heading`).
- **Oracle:** `inspect` attributes.

**QA-16.3 Escape in every context**
- **Steps:** exercise Escape with: search suggestions open (`.filter-input` focused with text), bulk mode with a selection, rectangle drag in progress (`drag` with a long hold), lightbox, lightbox with the delete confirmation, lightbox with the drawer open (QA-16.7), tag list, Settings page, a Settings dialog, and the nested duplicate delete dialog.
- **Expected:** only the top layer reacts per press: confirmation, then drawer/fullscreen, then lightbox; Settings Esc returns to the gallery only when no dialog is open; bulk Esc clears the selection when no modal is on top (`docs/frontend/architecture-and-ui-conventions.md#modal-and-overlay-conventions`).
- **Oracle:** `inspect` of `[role="dialog"]`, bulk panel `[data-testid="bulk-action-panel"]`, `.filter-input` listbox state, screenshots.

**QA-16.4 Search combobox keyboard**
- **Steps:** `fill .filter-input` a prefix, `keys ArrowDown`, `keys Enter` (inserts the suggestion), `keys Enter` (submits), `keys Control k`.
- **Expected:** the combobox/listbox roles are exposed, Ctrl+K focuses the input, Enter behavior as documented (`docs/subsystems/search-tags-and-media-groups.md`).
- **Oracle:** `inspect .filter-input` (`role`, `aria-expanded`, `aria-activedescendant`), `inspect '[role="listbox"]'`.

**QA-16.5 Timeline scrubber role and aria (large library)**
- **Steps:** if all results share one month, `qa_fs.py touch dataset/<photo>/<subfolder> --days-ago 400` on a few subfolders and `Rescan all`. `inspect '[data-testid="gallery-timeline"]'`; `click` it; `keys ArrowDown`, `ArrowUp`, `PageDown`, `PageUp`, `Home`, `End`; `drag '[data-testid="gallery-timeline"]' '[data-testid="gallery-timeline"]@0,120'`; `wait idle 30000`.
- **Expected:** `role="slider"`, `aria-orientation="vertical"`, name "Timeline", `aria-valuetext` with the month name or "No date"; keys step month/year/ends; a drag shows `[data-testid="gallery-timeline-label"]` and sets `data-gallery-timeline-dragging` until release (`docs/subsystems/library-query-and-gallery.md`).
- **Oracle:** `inspect` attributes after each key, screenshot mid-drag is not possible, so assert the label with `inspect` right after the drag command.

**QA-16.6 Position bar**
- **Steps:** try `inspect '[data-testid="gallery-scrollbar"]'`.
- **Expected:** gap: the "Gallery position" scrollbar (`role="scrollbar"`) exists only in windowed mode (more than 16,000,000 px of grid height), which this dataset never reaches; it is covered by the `gallery-100k` sweep and `gallery-scale` harness. Record the element as absent and file it as an unreachable gap.
- **Oracle:** the empty `inspect` result.

**QA-16.7 Resize and the lightbox drawer below 768 px**
- **Steps:** `ctl resize 700 800`, open a tile; `inspect '#lightbox-sidebar'` and screenshot; open the drawer with `button[aria-label="Open asset panel"]`; check the scrim `[data-testid="lightbox-sidebar-scrim"]`; press `keys Escape` (drawer first), click the scrim, navigate with the arrow keys; then `ctl resize 1200 800` and back to `700 800`. Also try `resize 320 240` and `1440 900`.
- **Expected:** below 768 px the sidebar is a closed drawer of width `min(340px, 100%)`; the media controls are inert while it is open; Escape and scrim close the drawer first; the explicit open/closed choice survives navigation and resize; no horizontal overflow at 320 px (`docs/subsystems/lightbox.md#toolbar-sidebar-confirmation-and-clipboard`).
- **Oracle:** `inspect '#lightbox-sidebar'`, `inspect '[data-testid="lightbox-sidebar-scrim"]'`, screenshots at each size.

**QA-16.8 Gallery layout at small sizes**
- **Steps:** at 320x240 and 700x800 inspect the top bar, tile grid, Settings page, and each modal; scroll the gallery with `wheel .gallery-scroll 1400`.
- **Expected:** no clipped primary controls; modals fit the viewport and scroll internally.
- **Oracle:** screenshots, `inspect` of bounding sizes where available.

**QA-16.9 Reduced overlays: lightbox auto-hide and keyboard reveal**
- **Steps:** open a tile, wait 4 s, screenshot; press any key.
- **Expected:** controls fade after 3 s of inactivity (opacity 0, no pointer input) and the first key press reveals them while still executing its action (`docs/subsystems/lightbox.md#toolbar-sidebar-confirmation-and-clipboard`).
- **Oracle:** two screenshots, `inspect '[data-testid="lightbox-action-rail"]'`.

**QA-16.10 Window controls**
- **Steps:** `inspect 'button[aria-label="Minimize window"]'`, `'button[aria-label="Maximize window"]'`, `'button[aria-label="Close window"]'`; the group has `aria-label="Window controls"`.
- **Expected:** the buttons exist with translated names. Maximize, Minimize and the minimum size cannot be proven without a window manager: record them as gaps and do not click Minimize or Maximize.
- **Oracle:** `inspect` results; gap entries.

**QA-16.11 Language smoke for accessibility names**
- **Steps:** `select '#settings-language-select' value=ja`, check the dialog and button names with `inspect`, switch back to `en`.
- **Expected:** aria-labels are translated, not left as English keys (`docs/development/localization.md`).
- **Oracle:** `inspect 'button[aria-label]'` samples.

**QA-16.12 Close window (last step)**
- **Steps:** after all evidence is collected, `click 'button[aria-label="Close window"]'`, then `ctl doctor` / `ctl logs`.
- **Expected:** the window closes and the session ends; the controller reports a stopped or unreachable app; the clerk then stops the session and runs the dataset guard (`docs/subsystems/settings-operations.md#application-and-view-lifetimes`). Do not continue driving after this step.
- **Oracle:** `doctor` output, `session.json` status, last `console.jsonl` entries.
