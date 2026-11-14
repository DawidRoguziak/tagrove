---
name: tauri-test-strategy
description: Entry point for writing, fixing, planning or reviewing tests in Tagrove (Tauri 2, Rust workspace of the core crate media_tagger_core and the adapter media_tagger, Solid 2 RC frontend). Picks the lowest test layer that proves the behavior (Vitest unit, Vitest + Testing Library, Rust unit, Rust integration targets, desktop WebdriverIO E2E), gives the working loop, the exact bun/cargo commands and a review checklist. Use for "write tests", "add a regression test", "which layer", "napisz testy", "testy jednostkowe/integracyjne/E2E".
---

# Testing strategy for Tagrove

Tagrove already has a complete, documented test setup. This skill routes you to it; it does
not introduce new tools. The owning page is `docs/development/testing.md` (read the section
for your layer, not all 1100 lines). CLAUDE.md, `.claude/rules/` and that page win over this
skill; if they disagree with it, follow them and fix this skill.

| Layer | Skill |
| --- | --- |
| Rust unit, command/service logic, serde contracts, `src-tauri/tests/` targets | `tauri-rust-tests` |
| Pure TS, Solid models and components, `src/platform/tauri` wrappers (Vitest) | `tauri-solid-tests` |
| Real desktop E2E (WebdriverIO + tauri-driver, `.e2e` profile) | `tauri-e2e-tests` |
| Interactive check of a UI change in the real app (clicks, screenshots) | `/verify-mediatagger` |

## 1. Pick the lowest layer that proves it

This is `docs/development/testing.md` → "Choosing the test level":

| Change | Minimum proof | Add when applicable |
| --- | --- | --- |
| Pure TS service, parser, reducer | Vitest unit | Component/model test if Solid ownership matters |
| Solid component, model, keyboard, modal, a11y | Vitest + Testing Library | Desktop E2E if it depends on WebView/Tauri |
| Rust helper, SQL, validation, scheduler, scan, thumbnail, backup | Rust unit | File-backed integration test for transactions / FS + SQLite |
| Backend workflow across DB modules | `backend_integration` (or Rust-only `backend_e2e`) | Desktop E2E if the JS boundary or UI is the contract |
| IPC command name/payload, serialization, event/channel, startup, profile | Rust test **plus** frontend API test | Desktop E2E for the real bridge |
| Destructive UI, FS mutation, import/export/restore, critical journey | Lower-layer edge cases | Isolated desktop E2E for the happy path and the safety guard |

A mocked `invoke` assertion never proves that Rust accepts the payload.

## 2. Working loop

1. Find the behavior and its layer. For a bug, write the failing regression test first and watch it fail.
2. Follow the existing tests next to the code (naming, helpers, mocks) instead of inventing a convention.
3. Run the narrowest command, then the whole affected layer.
4. Never weaken an assertion, add `skip`/`#[ignore]`, raise timeouts, or loosen an E2E guard to get green. Find the race.
5. Report exactly what ran and what is still unverified (for example desktop E2E not run because Xvfb/tauri-driver are unavailable).

## 3. Commands (the only ones to use)

```bash
# Frontend (Vitest via the package script: 8 GiB heap, 2 forks)
bun run test
bun run test -- src/features/lightbox/__tests__/LightboxModal.test.tsx
bun run test -- <file> -t "test name"
bun run test -- --maxWorkers=1          # out of memory

# Rust (always --locked)
bun run test:backend                    # unit + every src-tauri/tests target
cargo test --manifest-path src-tauri/Cargo.toml --locked <name>
cargo test --manifest-path src-tauri/Cargo.toml --locked --test backend_integration <name>
cargo test --manifest-path src-tauri/Cargo.toml --locked --test <target> <name> -- --ignored --nocapture --test-threads=1   # ignored diagnostics

# Desktop E2E: see tauri-e2e-tests (private Xvfb display, one run at a time)
bun run test:e2e:tauri --spec e2e/specs/app.smoke.e2e.js

# Static checks that often fail after test edits
bun run typecheck && bun run lint && bun run rust:clippy
```

Never use `npm`/`npx`/`pnpm`, plain `vitest`, or `cargo test` without `--locked`.

## 4. Things this project does not want

- **New test dependencies** (Playwright, `@wdio/tauri-service`, `tauri-plugin-wdio-*`, `tauri` `test` feature, rstest/proptest/insta/nextest, coverage plugins). They change `bun.lock`/`Cargo.lock` and are a significant-dependency decision that needs an ADR in `docs/adr/`. Propose and ask first; never add one as a side effect of writing a test.
- **New CI workflows.** `.github/workflows/privacy.yml` is intentionally the only workflow; tests run locally.
- **Refactoring production code only to make it testable** (for example moving `generate_handler!` into a new `configure()`), unless the user asked for it.
- **Real data.** Destructive tests use fresh temp roots only: never personal folders, the repo root, or real app data. Rust uses `tempfile` or in-memory SQLite.
- **Private content in fixtures.** The repo is public: synthetic fixtures and anonymized paths only.

## 5. Review checklist

- [ ] The test sits at the lowest layer that observes the behavior, next to the owning module.
- [ ] It asserts observable behavior (return value, DOM, persisted rows, files, IPC payload), not private details.
- [ ] Bug fixes come with a regression test that failed before the fix.
- [ ] Mocks and timers are reset; async ordering is explicit (deferred promises, `settle`, `flush`).
- [ ] Rust fixtures are unique and temporary; no shared fixed paths or process-global state.
- [ ] IPC changes are covered on both sides (Rust serde/command test + `src/platform/tauri/__tests__`), and `docs/architecture/ipc-contract.md` is updated.
- [ ] Changed UI copy or aria labels: E2E selectors and the `verify-mediatagger` recipes are updated in the same change.
- [ ] Destructive desktop flows stay inside temp roots and the `.e2e` profile; no guard was weakened.
- [ ] `docs/development/testing.md` is updated if a new test target, opt-in flag or convention was added.
