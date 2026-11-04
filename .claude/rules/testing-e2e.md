---
paths:
  - "e2e/**"
  - "src-tauri/tests/**"
  - "vitest.config.ts"
  - "src/test/**"
---

# Tests and desktop E2E

Read `docs/development/testing.md` (layer matrix, E2E isolation, opt-in flags, troubleshooting).

- Never weaken the guards in `e2e/build-e2e.js` (identifier, title, `target-e2e`), `e2e/wdio.conf.js` (cleans only `<XDG>/com.example.mediatagger.e2e`), or `e2e/control.js` (lock, owned temp root).
- One E2E build, suite, or `app:control` session at a time per checkout, because they share `dist/`, `target-e2e`, and ports 4444/4446/4447.
- Destructive specs index only fresh temporary roots. Specs start in English with the dark theme; translated scenarios switch language explicitly.
- Under Xvfb on a Wayland host, `unset WAYLAND_DISPLAY` and set `GDK_BACKEND=x11`. WebKit `:focus-visible` needs a native click. After a run, unmount FUSE mounts under `$XDG_RUNTIME_DIR/doc` and `/gvfs` before removing the temp root.
- New opt-in specs gate on `process.env.MEDIATAGGER_<FLAG> === "1"`. Add them to the opt-in table in testing.md.
- Vitest is memory-heavy (8 GiB heap per fork, 2 workers). Retry with `--maxWorkers=1`.
