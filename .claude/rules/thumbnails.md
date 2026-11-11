---
paths:
  - "src-tauri/src/thumbs.rs"
  - "src-tauri/src/services/thumb_*.rs"
  - "src-tauri/src/commands/thumbs.rs"
  - "src-tauri/src/db/thumbnails.rs"
  - "src/platform/tauri/thumbnails.ts"
  - "src/ui/controls/ThumbnailImage.tsx"
---

# Thumbnails

Read `docs/subsystems/thumbnails.md`.

- Cache key = SHA-256 of (`THUMB_CACHE_VERSION`, path, size, nanosecond mtime). Every DB write is a compare-and-set on that source version. If the key changes, or the renderer would produce a materially different picture, bump the version and plan cleanup (equivalent resampling or a nearby video keyframe did not bump it; ADR 0006).
- A rendered thumbnail is served only after the version-checked DB update succeeds. A rejected version answers 404 and stays retryable.
- `assets.thumb_path` is untrusted. Before deleting a file, canonicalize the path, reject symlinks and non-files, and require it to stay inside the thumbnail root.
- Locks: render and `thumb://` requests take the `thumb_lock` read guard per short section (never while a render runs or a source file is checked: a `thumb://` request stats its source between two sections and decides again from a fresh row), `clear_all_thumbnails` takes the write guard, and bulk cancel takes no lock (it only sets a flag).
- On-demand thumbnails are the `thumb://localhost/<id>?v=<thumb_version>` scheme (`services/thumb_protocol.rs`, ADR 0006), not a command. The parser accepts only a positive numeric ID, a 16-hex version and the `c`/`p`/`r` flags, never a path. `thumbnailUrl` in `src/platform/tauri/thumbnails.ts` is the only URL builder.
- Visible `thumb://` jobs are `Interactive` (LIFO) ahead of High (prefetch `p=1`, FIFO) and Low (bulk); WebKitGTK cannot cancel requests, so every request is answered. A tile whose prefetch has not arrived drops `p=1` once when it scrolls into view outside a pause, which promotes the queued job (ADR 0021).
- Gallery GIF tiles always use static thumbnails and never load the original GIF.
- Every ffmpeg/ffprobe run on a library video passes `LOCAL_VIDEO_INPUT_ARGS` (`-protocol_whitelist file` and a format list of single-file containers), so a manifest or script named as a video is not followed (ADR 0028).
