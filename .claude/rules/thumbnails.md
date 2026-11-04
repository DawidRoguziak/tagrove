---
paths:
  - "src-tauri/src/thumbs.rs"
  - "src-tauri/src/services/thumb_*.rs"
  - "src-tauri/src/commands/thumbs.rs"
  - "src-tauri/src/db/thumbnails.rs"
  - "src/library/thumbnails/**"
  - "src/ui/controls/ThumbnailImage.tsx"
---

# Thumbnails

Read `docs/subsystems/thumbnails.md`.

- Cache key = SHA-256 of (`THUMB_CACHE_VERSION`, path, size, nanosecond mtime). Every DB write is a compare-and-set on that source version. If the key or renderer changes, bump the version and plan cleanup.
- `ready` is sent only after the version-checked DB update succeeds. Rejected versions are `stale` and stay retryable.
- `assets.thumb_path` is untrusted. Before deleting a file, canonicalize the path, reject symlinks and non-files, and require it to stay inside the thumbnail root.
- Locks: render/ensure take the `thumb_lock` read guard, `clear_all_thumbnails` takes the write guard, and bulk cancel takes no lock (it only sets a flag).
- Only `ensure_thumbnails` (streamed) is registered for on-demand gallery thumbnails.
- Gallery GIF tiles always use static thumbnails and never load the original GIF.
