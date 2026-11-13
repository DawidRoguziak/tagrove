# Gallery and search

The gallery shows indexed media; the search field filters it by tags and supported query syntax.

## Sub-features

- `search-empty`: submit an absent tag and see no results.
- `search-clear`: restore all seeded tiles.
- `gallery-open`: open a visible tile in the lightbox.
- `gallery-timeline`: click, drag, and key the month scrubber.
- `gallery-select`: Shift/Ctrl selection and the rectangle selection in bulk mode.
- `gallery-tile-size`: Ctrl+wheel on the gallery changes the tile size.

## How to get to it (user POV)

The gallery is the initial view. From settings, click Back. From a lightbox, close the preview.

## Driving it with WebdriverIO

Preconditions: doctor succeeds, the fresh seven-fixture library is present, and the gallery is open.

```sh
bun run app:control fill "$control_session" .filter-input absent-control-tag
bun run app:control keys "$control_session" Enter
bun run app:control inspect "$control_session" .filter-input
bun run app:control screenshot "$control_session"
bun run app:control click "$control_session" 'button[aria-label="Clear all search filters"]'
bun run app:control inspect "$control_session" 'button[data-asset-id]'
bun run app:control screenshot "$control_session"
```

Require the submitted input value and `No results`, then an empty input and seven restored
tiles. If results are still loading, inspect again; do not infer completion from a click response.

## Gotchas

Search uses tag/query grammar, not arbitrary filename matching. Fixture IDs and tile ordering
are not fixed. Obtain current IDs from `inspect` before opening a specific media kind.

Tiles (`button[data-asset-index]`) and placeholders (`div[data-asset-index]`) sit in recycled row
elements (`[data-gallery-row]`) inside `[data-testid="gallery-grid-content"]`. Take grid width
and margin from that content element, not from `tile.parentElement`. Results taller than the
16,000,000 px physical window scroll inside that window: `scrollTop = 0` is only the window start.
Use End/Home or the `role="scrollbar"` named "Gallery position" to reach the ends.

The month timeline is a `role="slider"` named "Timeline" (`[data-testid="gallery-timeline"]`).
It appears only when the result spans at least two months, so the seven fixtures, which share
one file time, normally do not show it (spread file times in a disposable copy first). Its
`aria-valuetext` is the month of the first visible item or "No date"; ArrowDown/ArrowUp step a
month, PageDown/PageUp a year, Home/End the ends. It sits beside the position bar and never
replaces "Gallery position". Drive it with the controller:

```sh
bun run app:control wait "$control_session" '[data-testid="gallery-timeline"]' visible
bun run app:control click "$control_session" '[data-testid="gallery-timeline"]'
bun run app:control keys "$control_session" End
bun run app:control inspect "$control_session" '[data-testid="gallery-timeline"]'
bun run app:control drag "$control_session" '[data-testid="gallery-timeline"]' '[data-testid="gallery-timeline"]@0,120' 300
bun run app:control inspect "$control_session" '[data-testid="gallery-timeline-label"]'
bun run app:control wait "$control_session" idle 30000
```

A drag uses pointer capture and pauses page and thumbnail requests until release (or 150 ms of
no movement); `data-gallery-timeline-dragging` is set meanwhile and the label
(`gallery-timeline-label`) shows the month. Require `aria-valuetext` to change after the key or
drag, and the grid to settle after `wait idle`.

The tile loader (`[data-testid="tile-thumb-loader"]`) is always in the DOM and only becomes
visible after 300 ms, so wait for thumbnails with `wait ... hidden`, never `gone`:

```sh
bun run app:control wait "$control_session" '[data-testid="tile-thumb-loader"]' hidden 30000
```

Tile size is `#tile-size` (range 140-340, step 8, not persisted); Ctrl+wheel over the gallery
changes it by 8 per step:

```sh
bun run app:control wheel "$control_session" .gallery-scroll -8 ctrl
bun run app:control inspect "$control_session" '#tile-size'
```

If WebKit does not report `ctrlKey` for the WebDriver wheel, use
`native wheel <x> <y> <steps> ctrl` on screen coordinates from a screenshot.

In bulk mode (`button[aria-label="Enable bulk actions"]`), `keys Control a` selects every result
as one range; the bulk header shows `Selected: <total>`. Selections larger than 10,000 items are
cleared with a "Selection cleared" alert (`[data-testid="bulk-selection-cleared"]`) when the
result changes (a new search, a scan, a regroup). The same alert says "The selection expired before
it could be used" when the session was already gone at selection time or a refresh failed.

Selection gestures need bulk mode. `modclick` holds the modifier and always releases it:

```sh
bun run app:control click "$control_session" 'button[aria-label="Enable bulk actions"]'
bun run app:control click "$control_session" 'button[data-asset-index="1"]'
bun run app:control modclick "$control_session" Shift 'button[data-asset-index="5"]'
bun run app:control modclick "$control_session" Control 'button[data-asset-index="8"]'
bun run app:control modclick "$control_session" Control+Shift 'button[data-asset-index="12"]'
bun run app:control inspect "$control_session" '[data-testid="bulk-action-panel"]'
```

Shift+click replaces the selection with a range, Ctrl+Shift+click adds one, Ctrl+click toggles.
The rectangle needs at least 150 ms of hold and 5 px of movement, which `drag` provides;
the rectangle element is `[data-testid="gallery-selection-rectangle"]` while dragging:

```sh
bun run app:control drag "$control_session" 'button[data-asset-index="0"]' 240,160 400
bun run app:control inspect "$control_session" '[data-testid="bulk-action-panel"]'
bun run app:control keys "$control_session" Escape
```

Require `Selected: N` in the panel; Escape clears the selection. Use `dblclick` when a recipe
needs a double click (for example excluding a tag in the tag list).
