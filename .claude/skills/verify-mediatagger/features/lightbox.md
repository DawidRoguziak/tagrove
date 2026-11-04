# Lightbox

Opening a gallery tile shows its media and editing controls. Image content should be visible
in a screenshot; native video requires separate visual verification.

## Sub-features

- `lightbox-open`: open a fixture through a gallery tile.
- `lightbox-close`: use Close preview or Escape.
- `lightbox-tag`: add a tag and confirm it remains after reopening.
- `lightbox-video-capture`: inspect native video separately from image rendering.

## How to get to it (user POV)

Click a gallery tile. Close with the preview's close button or Escape.

## Driving it with WebdriverIO

Preconditions: gallery open with fixtures. Inspect the current tiles and choose a copied PNG
for the image/tag recipe. Use its returned asset ID in the selector below as `asset_id`.

```sh
bun run app:control inspect "$control_session" 'button[data-asset-id]'
bun run app:control click "$control_session" "button[data-asset-id='$asset_id']"
bun run app:control inspect "$control_session" '[role="dialog"]'
bun run app:control screenshot "$control_session"
bun run app:control fill "$control_session" '#lightbox-tag-draft-input' control-proof
bun run app:control keys "$control_session" Enter
bun run app:control inspect "$control_session" '[data-testid="lightbox-tag-panel"]'
bun run app:control click "$control_session" 'button[aria-label="Close preview"]'
bun run app:control click "$control_session" "button[data-asset-id='$asset_id']"
bun run app:control inspect "$control_session" '[data-testid="lightbox-tag-panel"]'
bun run app:control keys "$control_session" Escape
```

Require visible image pixels and the tag `control-proof` after reopening. Repeat open/close
with the close button and Escape when verifying both entry points. For video, choose an MP4
tile, save a screenshot, and inspect both the native controls and actual picture area.

## Gotchas

The controller cannot click native GTK playback controls or read their labels through DOM
inspection. A black picture area is not proof of rendered video, even if native controls are
present. Do not change application playback behavior to make a virtual-display proof pass.
The fixture's runtime is eight seconds. Capture promptly after opening.
