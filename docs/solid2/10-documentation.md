# Stage 10: Documentation

Previous: [parity and cutover](09-parity-and-cutover.md). This is the final stage.
Read the [architecture and completion rules](README.md) and update [progress](PROGRESS.md).

## Outcome

Make the canonical documentation describe the implemented Solid application, its file
ownership, operational contracts, development setup, and verified behavior.

## Documentation work

- Rewrite the frontend architecture guide around the final directory structure, model
  interfaces, application/view/session lifetimes, reactivity, and cleanup rules.
- Update the system overview's frontend lifecycle and diagrams. Check descriptions of
  eager/lazy features against imports rather than carrying forward stale statements.
- Update IPC implementation links and type locations while keeping wire semantics clear.
- Update gallery/query, thumbnails, search/tags/groups, lightbox, settings, and data-safety
  pages where their frontend owners or orchestration changed.
- Update setup/build guidance, Node requirements, pinned RC versions, compiler/test
  configuration, and the virtual-core adapter rationale.
- Update localization guidance for the Solid i18next binding. Preserve the resource and
  locale-validation contracts.
- Update the testing guide with actual commands, Solid test patterns, the migrated
  behavior inventory, desktop opt-in requirements, and remaining verification limits.
- Update the AGENTS documentation index and stack description to point to final owners.
  Preserve the worktree prohibition, privacy rules, and profile isolation requirements.
- Remove obsolete React guidance from current architecture instructions. Historical
  verification records may retain accurate descriptions of the React version they tested.
- Complete the migration progress record with actual stage results and references.
  Do not turn expected outcomes into claims of executed tests.

## Verification and acceptance

- Check relative links, source paths, command names, package versions, and examples
  against the final tree and executable configuration.
- Run affected tooling checks and the documented Solid build and isolated desktop smoke
  workflow. Record the exact commands and outcomes.
- Distinguish implemented guarantees, intended conventions, and tested behavior.
- Keep logs, media, databases, and desktop screenshots outside tracked documentation.
- Add no implementation changes merely to satisfy a prose claim. If a code defect is
  discovered, fix and test it before treating its documentation as complete.

## Review and local commit

Self-review the documentation against the actual application and test evidence, fix
incorrect claims or links, and apply the complete stage gate from the README. This
documentation-only stage validates references and documented workflows instead of
adding tests that merely assert wording.

Intended commit: `docs: document Solid architecture and verification`.
