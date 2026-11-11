---
paths:
  - "src/features/lightbox/**"
  - "src/app/LightboxBoundary.tsx"
  - "src/platform/tauri/video.ts"
  - "src-tauri/src/commands/video.rs"
  - "src-tauri/src/services/video_*.rs"
  - "src-tauri/src/video_surface.rs"
---

# Lightbox and native video

Read `docs/subsystems/lightbox.md`. For video work, also read the IPC (`openVideo`, `MpvVideoEvent`) and frontend layer sections.

- Every `select` (including close) increments `selectionRequest`. A detail response may replace the selection only if its request ID is still current.
- Navigation uses the query's `total` and the global index, wrapping modulo count. A failed page rolls back and shows Retry.
- `createLightbox` is the only owner of tag-save serialization and coalescing.
- Video: one process-wide libmpv player drawn on a GTK GLArea above the WebView. The DOM controls must stay outside the native surface bounds (`measureNativeVideoBounds`). The backend authorizes the canonical path from the asset ID.
- libmpv flags are read through the private `c_int` adapter, not the generic `bool` getter.
- `PLAYER_OPTIONS` and `SCRIPT_OPTIONS` keep libmpv on the one authorized file: no playlist, manifest or script content is followed, no `yt-dlp` hook, no user mpv configuration (ADR 0028). Do not loosen them without a new ADR.
- Native video changes need a real desktop check (`MEDIATAGGER_NATIVE_VIDEO=1` specs). Xvfb screenshots of MP4 pixels are unreliable.
