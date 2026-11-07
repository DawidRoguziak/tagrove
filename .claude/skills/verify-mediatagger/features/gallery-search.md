# Gallery and search

The gallery shows indexed media; the search field filters it by tags and supported query syntax.

## Sub-features

- `search-empty`: submit an absent tag and see no results.
- `search-clear`: restore all seeded tiles.
- `gallery-open`: open a visible tile in the lightbox.

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
one file time, normally do not show it. Its `aria-valuetext` is the month of the first visible
item; ArrowDown/ArrowUp step a month, PageDown/PageUp a year, Home/End the ends. It sits beside
the position bar and never replaces "Gallery position".
