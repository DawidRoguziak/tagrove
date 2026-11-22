# E13 review: open findings

Epic: E13 "Modal listy tagów i wspólny TagEditor"
Date: 2026-11-22
Review base: bded9c58b03c699b037dcf87d5c544dabbef5e85
Review rounds: 6 (8 confirmed findings fixed, 0 rejected; the findings below stayed uncertain and were then decided by measurement, see "Measurement and fix")

| id | priority | path:line | title | status |
| --- | --- | --- | --- | --- |
| r2-perf-1 | P3 | src-gpui/crates/media_tagger_gpui/src/views/tag_list.rs:610 | Tag list dialog rebuilds and lays out every loaded chip on each scroll or hover frame | fixed (confirmed by measurement, P2) |
| r4-gpui-3 | P3 | src-gpui/crates/media_tagger_gpui/src/views/tag_list.rs:617 | Every scroll frame of the tag list rebuilds and lays out every loaded chip | fixed (confirmed by measurement, P2) |
| r6-gpui-1 | P3 | src-gpui/crates/media_tagger_gpui/src/views/tag_list.rs:692 | Tag list rebuilds and lays out every loaded chip on each scroll event | fixed (confirmed by measurement, P2) |

The three findings describe one problem, reported by different lenses in different rounds.

## r2-perf-1, r4-gpui-3, r6-gpui-1 (fixed; confirmed by measurement as P2)

Trigger: open the tag list in a library with many distinct tags and scroll until dozens of pages of 100 tags are loaded (thousands of chips), then keep scrolling or move the pointer over the list.

Impact: each scroll-wheel event or chip hover notifies the view (gpui div.rs, `cx.notify(current_view)` on scroll offset change), so `TagListModal::render` runs again. It builds one element per loaded tag (a div, two `cx.listener` closures, a focus-handle lookup, styles), and taffy lays out the whole flex-wrap list, so the cost per frame grows with the number of loaded tags instead of the visible range. Solid keeps the same DOM node count, but the browser scrolls it without laying it out again. Verifiers rate it P3 at most: reaching thousands of chips takes about 50 scroll-to-end loads, typing in the filter resets the list, and per-chip work is small (labels are cached in `refresh_rows`).

Suggested fix: pack chips into rows from measured widths (cached per page_version and width) and render only rows in the visible range plus overscan, with spacers, keeping the focused chip mounted; or move the chip list into its own child entity notified only when rows change; a smaller step is building the click and exclude handlers once per row in `ChipRow`.

Evidence:
- src-gpui/crates/media_tagger_gpui/src/views/tag_list.rs: `render` maps all of `list.page().items` to chip elements with no visible-range cut; `chip()` creates two listeners and a HashMap lookup per chip.
- gpui (pin d01b134) `crates/gpui/src/elements/div.rs` around lines 3443-3488: the scroll listener calls `cx.notify(current_view)` on offset change.
- `sync()` also calls `cx.notify` on every runtime publish while open.
- `search/tag_list.rs` `page_loaded`: `merge_tag_pages` grows the shown list without bound until a new filter, vocabulary or close.
- Counterevidence: `TagListChipList.tsx` also renders every loaded tag with `<For>`; the existing test counts only label builds, not elements or layout; no frame time was measured.

Missing fact: measured render, layout and prepaint frame time in a release build while scrolling and hovering the tag list with about 1,000, 5,000 and 20,000 loaded chips. Reject if well under 8 ms; confirm as P3 or P2 above 16 ms.

## Measurement and fix

Verdict: confirmed. With every loaded tag drawn, a frame took 11 ms at 1,000 loaded tags,
57 ms at 5,000 and 406 ms at 20,000, growing faster than the number of tags; 5,000 tags
are 50 scroll-to-end loads, which a user can reach, so the finding is P2 (above 16 ms),
not P3. Fixed in this epic; nothing of it stays open.

Measurement: `frame_time_while_scrolling_and_hovering_a_long_tag_list` in
`src-gpui/crates/media_tagger_gpui/tests/tag_list_window.rs` (ignored), release build:

```bash
cargo test --manifest-path src-gpui/Cargo.toml --locked --release -p media_tagger_gpui --test tag_list_window frame_time -- --ignored --nocapture
```

The tags are loaded as a user loads them, by scrolling the list to its end, from a
`FakeBackend` vocabulary of 20,000 synthetic names (100 per page). Each size runs 300
frames of an unchanged redraw, of wheel scrolling 40 px a frame (down for 50 frames, then
up, far from the end so nothing loads), and of the pointer moving over the tags. A frame
is the event, the work it queues and `Window::draw` on GPUI's test platform: element
tree, layout, prepaint and paint into a scene. Not included: GPU work and real text
shaping (the test platform shapes with GPUI's `NoopTextSystem`, one em per character),
so real frames cost more, not less.

| loaded tags | frame | before: mean / p95 / max (ms) | after: mean / p95 / max (ms) |
| --- | --- | --- | --- |
| 1,000 | redraw | 5.69 / 5.73 / 5.84 | 0.90 / 0.91 / 0.95 |
| 1,000 | scroll | 11.37 / 11.46 / 12.62 | 1.98 / 2.02 / 2.05 |
| 1,000 | hover | 9.61 / 11.42 / 11.48 | 1.53 / 1.83 / 1.91 |
| 5,000 | redraw | 28.26 / 28.71 / 32.81 | 0.91 / 0.92 / 1.03 |
| 5,000 | scroll | 56.58 / 57.35 / 70.61 | 1.99 / 2.04 / 2.06 |
| 5,000 | hover | 48.09 / 57.74 / 61.21 | 1.55 / 1.86 / 1.89 |
| 20,000 | redraw | 202.79 / 205.51 / 223.36 | 0.90 / 0.92 / 0.98 |
| 20,000 | scroll | 406.46 / 410.73 / 427.81 | 1.97 / 2.02 / 2.05 |
| 20,000 | hover | 345.60 / 412.50 / 421.98 | 1.53 / 1.83 / 1.88 |

After the fix a frame draws 62 buttons at every size. Loading the 20,000 tags took 184 s
of test time before (every load frame laid out all tags) and 1.9 s after.

Fix: the view packs the buttons into rows as `flex flex-wrap gap-1.5` does
(`media_tagger_client::search::tag_list::rows::ChipRows`), from each tag's text width,
measured with the window's text system once per new page and kept per rows version and
list width. A frame draws the rows that reach into the visible part with three rows of
overscan on each side, plus the row of the focused button and the rows next to it, so the
focus, Tab, Shift+Tab, Shift+Enter, Shift+Space and the scroll keys work wherever the list
is scrolled; the rows not drawn take their height as margin, so the content height and
the 120-pixel pagination stay those of the whole list. Focus reveal is worked out from
the packed rows instead of the drawn bounds. Before the list's first layout its width is
unknown, so that frame draws the buttons in one flex-wrap and the next packs them, as
does a change of width. No plan decision was needed: the packing is the tag list's own
and no shared virtual list primitive was made.

Tests that prove it: `a_frame_draws_the_rows_in_view_whatever_the_number_of_loaded_tags`
(fails with every row drawn: "100 of 100 tags drawn") and
`the_drawn_rows_are_the_rows_of_flex_wrap` in `tests/tag_list_window.rs`, and the unit
tests of `search/tag_list/rows.rs`. The existing window tests (pagination at 120 px, the
click timer, Shift+Enter and Shift+Space, keyboard reach of tags outside the visible area
from r4-gpui-1) pass unchanged.

Memory of the loaded tags (not changed, reported): the model's unbounded growth costs
about 330 bytes of heap per loaded tag in the test window (synthetic names of 16 bytes on
average: the model's name, the view's button state and label, the packed rows, the focus
handles of drawn buttons), measured once with a counting allocator between 1,000 and
20,000 loaded tags. That is about 6.5 MB at 20,000 and about 33 MB at 100,000 tags, the
vocabulary size the search epic measured; reaching 100,000 takes 1,000 scroll-to-end
loads, and a new filter, a new vocabulary or closing frees it. Not a memory problem. Each
page load still merges the whole shown list again (`merge_tag_pages`, linear in the shown
tags), which is a cost per load, not per frame.

Open after this: nothing. No P0 or P1 finding is open for E13.
