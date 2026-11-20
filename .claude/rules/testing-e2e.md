---
paths:
  - "e2e/**"
  - "src-tauri/tests/**"
  - "vitest.config.ts"
  - "src/test/**"
  - "src-tauri/crates/media_tagger_core/src/test_support.rs"
---

# Tests and desktop E2E

Read `docs/development/testing.md` (layer matrix, E2E isolation, opt-in flags, troubleshooting).

- Never weaken the guards in `e2e/build-e2e.js` (identifier, title, `target-e2e`), `e2e/wdio.conf.js` (cleans only `<XDG>/com.example.mediatagger.e2e`), `e2e/control.js` (lock, owned temp root), or `e2e/compare/gpui-control-support.js` (GPUI identity, lock, owned scratch root, desktop-conflict checks).
- One E2E build, suite, `app:control` session, `gpui:control` session, or `compare:ui` run at a time per checkout, because they share `dist/`, the `target-e2e` directories, and ports 4444/4446/4447. `compare:ui` and `gpui:control` refuse to start beside the others; the Tauri commands do not check `artifacts/gpui-control/active.lock`, so check it yourself first.
- Destructive specs index only fresh temporary roots. Specs start in English with the dark theme; translated scenarios switch language explicitly.
- Under Xvfb on a Wayland host, `unset WAYLAND_DISPLAY` and set `GDK_BACKEND=x11`. WebKit `:focus-visible` needs a native click. After a run, unmount FUSE mounts under `$XDG_RUNTIME_DIR/doc` and `/gvfs` before removing the temp root.
- New opt-in specs gate on `process.env.MEDIATAGGER_<FLAG> === "1"`. Add them to the opt-in table in testing.md.
- Vitest is memory-heavy (8 GiB heap per fork, 2 workers). Retry with `--maxWorkers=1`.
