# Solid 2 deep review

This directory contains the plan only. Start the review only after a separate user request.

Review the entire Solid frontend for correctness, performance and stability. Follow IPC into Rust, SQLite and native playback as necessary. Review current source including existing uncommitted edits. Application code and existing tests are read-only. Do not create worktrees, fix code, upgrade dependencies or push. Commit only when the user explicitly requests it.

[PROGRESS.md](PROGRESS.md) is the single source of truth for the current step. Every stage starts pending; planning is not review completion. Resume by comparing source hashes and checking unfinished assignments. Update progress at stage starts, assignment completion, blockers and handoffs.

Use the repository deep-review record contract, two independent perspectives per area, evidence exchange after saved initial reviews and a fresh verifier per distinct claim. At most three workers run concurrently. The coordinator serializes desktop sessions and shared builds. Use synthetic fixtures and disposable profiles exclusively. Raw evidence belongs under ignored raports/; tracked summaries must contain no private data.

| Stage | Focus |
| --- | --- |
| 00 | [Baseline and coverage map](00-baseline-coverage-map.md) |
| 01 | [Solid 2 semantics and toolchain](01-solid-2-semantics-toolchain.md) |
| 02 | [Test reliability](02-test-reliability.md) |
| 03 | [Bootstrap and application lifetime](03-bootstrap-application-lifetime.md) |
| 04 | [Query sessions and page caching](04-query-sessions-page-caching.md) |
| 05 | [Metadata and mutation coordination](05-metadata-mutation-coordination.md) |
| 06 | [Gallery virtualization and selection](06-gallery-virtualization-selection.md) |
| 07 | [Thumbnail pipeline](07-thumbnail-pipeline.md) |
| 08 | [Search and suggestions](08-search-suggestions.md) |
| 09 | [Tag editing, bulk actions, and groups](09-tag-editing-bulk-actions-groups.md) |
| 10 | [Lightbox model and image controls](10-lightbox-model-image-controls.md) |
| 11 | [Native video integration](11-native-video-integration.md) |
| 12 | [Settings and maintenance](12-settings-maintenance.md) |
| 13 | [Shared UI, overlays, and localization](13-shared-ui-overlays-localization.md) |
| 14 | [Integrated performance and stability](14-integrated-performance-stability.md) |
| 15 | [Independent verification and final report](15-independent-verification-final-report.md) |

Completion requires dual independent coverage for every included file, accounted-for collaboration, independent dispositions for all candidates, required runtime investigations and a passing review-record validator. Missing coverage or unperformed required runtime work makes the report partial. Keep unresolved investigated claims explicit. Findings include trigger, expected/actual behavior, impact, root cause, reproduction, counterevidence, verdict, repair direction and proposed regression without implementing them. Performance findings need a measured effect plus mechanism.
