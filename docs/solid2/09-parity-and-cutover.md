# Stage 9: Parity, cutover, and React removal

Previous: [settings and maintenance](08-settings-and-maintenance.md).
Next: [documentation](10-documentation.md).
Read the [architecture and completion rules](README.md) and update [progress](PROGRESS.md).

## Outcome

Prove behavioral and visual parity, make Solid the normal application entry, and remove
the temporary React implementation and migration machinery.

## Implementation

- Resolve every entry in the test behavior inventory. Keep equivalent behavioral
  assertions; replace React-specific lifecycle assertions with Solid ownership checks.
- Run the same desktop scenarios against React and Solid serially, with fresh equivalent
  synthetic media and E2E app data. Compare the same settled UI states and geometry.
- Compare gallery, search, tagging, bulk, lightbox, settings, confirmations, errors,
  loading, native video, and reload persistence. Fix unexplained visible differences.
- Compare vocabulary/query IPC counts, cache bounds, mounted tiles, measurement rebuilds,
  and performance samples on the same host. Retain existing deterministic bounds.
- Make the normal HTML entry, Vite config, TypeScript config, tests, and package commands
  use Solid. Keep `bun run tauri:dev` and the existing dev/E2E/release profiles.
- Remove React source, React dependencies/types, React-i18next, React Virtual, the React
  compiler, temporary API re-exports, renderer selection, and obsolete test configuration.
- Remove React-specific lint overrides. Keep applicable correctness checks and ensure
  the retained checks cover the final source locations.
- Remove temporary compatibility props and unused wrappers after updating callers.
  Avoid unrelated backend or UI changes in the cleanup.

## Tests and acceptance

- Run `bun run quality`, the full frontend and Rust suites, both pre-cutover builds,
  and the final Solid build. Run packaging/tooling tests when changed inputs affect them.
- Run `bun run test:all` with the documented isolated Linux desktop setup.
- Explicitly run the relevant opt-in desktop suites: gallery performance/selection,
  thumbnail fade, UI geometry/footer, tag editor, native video/speed, lightbox activity,
  animations, settings language, and About. Run the supported reduced-motion variants.
  A default desktop run alone does not cover these scenarios.
- Verify both themes, English/Polish, locale validation for all languages, narrow layouts,
  focus/keyboard behavior, lazy-load failures, and cleanup across repeated navigation.
- Inspect the final production dependency graph and output for React or legacy entry code.
- Confirm the test inventory has no unexplained gaps and report platform-specific limits.

## Review and local commit

Review the final application, dependency graph, removal diff, parity evidence, and test
inventory. Apply the complete stage gate from the README before declaring cutover complete.

Intended commit: `refactor: complete Solid migration and remove React`.
