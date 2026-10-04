# Stage 7: Native video integration

Previous: [lightbox and images](06-lightbox-and-images.md).
Next: [settings and maintenance](08-settings-and-maintenance.md).
Read the [architecture and completion rules](README.md) and update [progress](PROGRESS.md).

## Outcome

Connect the Solid lightbox to the existing libmpv/GTK playback backend while preserving
session ownership, native bounds, controls, and recovery behavior.

## Implementation

- Port the native playback model under `features/lightbox/video`. Keep video transport
  payloads and command wrappers in the platform boundary.
- Preserve begin/open/cancel tokens, bounded early-event buffering, returned-session
  filtering, supersession, and cleanup after async open completes on a retired owner.
- Coalesce native bounds updates with the existing sequencing and acknowledgment rules.
  Do not expose DOM controls beneath an uncleared GTK surface.
- Preserve sidebar clearance, the windowed top control reservation, fullscreen geometry,
  pointer-activity handling, and bounds-sensitive focus restoration.
- Preserve translated control labels, playback settings, speed, seeking, pause/mute,
  failure UI, and reopening/replacement behavior.
- Keep backend and GTK implementation behavior unchanged. Do not replace native playback
  with an HTML video implementation.

## Tests and acceptance

- Port native-session and native-layout tests, including late events, cancel/open races,
  failed bounds updates, label updates, replacement, and disposal.
- Run relevant Rust video tests, including `cargo test --manifest-path src-tauri/Cargo.toml video_ --lib`.
- Run native playback, speed, fullscreen, sidebar, and activity desktop scenarios on an
  authenticated private display with disposable app data and generated media.
- Verify repeated same-file reopening, different-file replacement, rate retention,
  corruption/failure recovery, resize, close during open, and old-session rejection.
- Distinguish real native pixel/control checks from mocked DOM or null-output libmpv
  tests. Record missing native coverage rather than reporting it as passed.

## Review and local commit

Review session identity, async cleanup, bounds acknowledgment, native focus, and retained
settings. Apply the complete stage gate from the README.

Intended commit: `feat: migrate native video lifecycle to Solid`.
