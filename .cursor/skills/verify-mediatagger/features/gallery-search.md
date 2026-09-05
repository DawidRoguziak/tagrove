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
