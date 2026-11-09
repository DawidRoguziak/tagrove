# tauri-testing (Tagrove)

Testing skills adapted to this repository. They route to the existing setup and do not add tools:

- `tauri-test-strategy`: choosing the lowest test layer, working loop, the exact `bun`/`cargo --locked` commands, review checklist.
- `tauri-rust-tests`: `#[cfg(test)]` unit tests, serde IPC contract tests, `tempfile`/in-memory SQLite fixtures, `src-tauri/tests/` targets.
- `tauri-solid-tests`: Vitest + `@solidjs/testing-library` on Solid 2 RC (`flush`, `settle`, `renderSolid`), IPC mocked with `vi.mock` of `src/platform/tauri`.
- `tauri-e2e-tests`: the guarded WebdriverIO + tauri-driver desktop E2E on a private Xvfb display.

They load automatically for testing tasks; call one directly with `/tauri-testing:<skill-name>`.
`docs/development/testing.md`, `CLAUDE.md` and `.claude/rules/` take precedence over these skills.

Adapted from a generic Tauri 2 testing kit. Re-running that kit's `install.sh --force` overwrites these edits.
