# Lightbox

Opening a gallery tile shows its media and editing controls. Image content should be visible
in a screenshot; native video requires separate visual verification.

## Sub-features

- `lightbox-open`: open a fixture through a gallery tile.
- `lightbox-close`: use Close preview or Escape.
- `lightbox-tag`: add a tag and confirm it remains after reopening.
- `lightbox-video-capture`: inspect native video separately from image rendering.
- `lightbox-video-controls`: drive the GTK video controls with `native` and read the player state.
- `lightbox-delete`: delete media through the inline confirmation, including the missing-file alert.

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

### Native video controls

The GTK controls (play, seek, mute, volume, speed popover, fullscreen) are not DOM elements and
have no selectors. Take a screenshot, read the PNG, and drive them with `native` on screen
coordinates (1440x900 display). The player container keeps its state in attributes, so verify
the effect with `inspect`:

```sh
bun run app:control wait "$control_session" '[data-lightbox-video-player]' displayed
bun run app:control screenshot "$control_session"
bun run app:control native "$control_session" click 720 860
bun run app:control inspect "$control_session" '[data-lightbox-video-player]'
bun run app:control click "$control_session" '[data-lightbox-video-player]'
bun run app:control keys "$control_session" ArrowRight
bun run app:control keys "$control_session" f
bun run app:control keys "$control_session" Escape
```

`inspect` shows `data-native-paused`, `data-native-time`, `data-native-muted`,
`data-native-volume`, `data-native-rate`, and `data-native-fullscreen`. Clicking the picture
pauses and focuses the container; with that focus Space/Enter toggle pause and Left/Right
seek 5 s, otherwise Left/Right navigate to the previous/next result. Escape closes the speed
popover first, then fullscreen, then the lightbox. After every `native` command the
controller runs doctor; a failed doctor taints the session.

### Delete and the alert

```sh
bun run app:control click "$control_session" 'button[aria-label="Delete media"]'
bun run app:control fill "$control_session" '#lightbox-delete-confirm-input' Yes
bun run app:control click "$control_session" 'button=Confirm'
bun run app:control doctor "$control_session"
```

The confirmation word is localized ("Yes" in English, "Tak" in Polish; the comparison ignores
case). Escape closes only the inline confirmation. If the source file was already missing or
its cleanup is pending, the app raises a `window.alert` and doctor reports `alert-open`: read
it with `alert "$control_session" text`, then `alert "$control_session" accept`. Verify the
file, the database row, and the thumbnail are gone, not just the tile. Use only disposable
media.

## Gotchas

Native GTK playback controls are reached with `native` and screen coordinates, and their
effect is read from the `data-native-*` attributes, not from labels. A black picture area is
not proof of rendered video, even if native controls are present (MP4 pictures can be black
on Xvfb). Do not change application playback behavior to make a virtual-display proof pass.
The fixture's runtime is eight seconds. Capture promptly after opening.
