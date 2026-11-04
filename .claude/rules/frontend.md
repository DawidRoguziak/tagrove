---
paths:
  - "src/app/**"
  - "src/ui/**"
  - "src/styles.css"
  - "index.html"
---

# Frontend composition and UI conventions

Read `docs/frontend/architecture-and-ui-conventions.md`.

- Import direction: `ui` must not import `features` or `library`; `library` must not import `features`; `platform` must not import `features` or `ui`. Only `src/platform/tauri` talks to Tauri.
- `createAppRuntime` owns all models for the app's lifetime. Views remount and lose local and DOM state. Settings and lightbox are lazy and prefetched after 1.5 s.
- Models expose read-only getters and explicit commands. They publish state only after backend success, using the canonical result. Don't add production `flush()` calls to fix sequencing.
- Overlays register with `UiLayerProvider` and use `UiModal`. Keep modal owners mounted when `open` becomes false. Don't add standalone global Escape listeners.
- Styling: prefer semantic DaisyUI colors and the app tokens (`--surface-*`, `--border-*`, `--radius-*`, `--shadow-*`). Never animate dimensions, spacing, or layout. Use `motion-enter` for opacity-only entrances.
- Only theme and language are persisted (localStorage `media-tagger.theme` / `media-tagger.language`).
- This is Solid 2, not React: no hooks or rerender model. Use owned `renderHook` for models and Testing Library for components (`src/test/renderSolid.tsx`, `settle.ts`).
