# 13 Shared UI, overlays, and localization

Initial status: pending. Current status is recorded only in [PROGRESS.md](PROGRESS.md).

## Source scope

src/ui, src/i18n, styles, utility callers. Expanded paths are in the run coverage record.

## Relevant documentation

frontend/architecture-and-ui-conventions.md; development/localization.md. Paths are relative to docs unless explicitly named as skill records. Read linked contracts needed by the investigation.

## Investigation questions

Inspect retained/nested modals, focus, inertness, scroll locks, interrupted motion, reduced motion, portals, language and persistence errors.

## Reproduction scenarios

Nested open/close, interrupted exit, language switch and unavailable storage; inspect shared controls, DOM helpers and styles.

## Required evidence

Two independent reviewer identities and perspectives, inspected paths, commands with results, callers and guards, saved initial findings before collaboration, response-order or runtime evidence where relevant, source hashes and independent candidate verdicts. Keep all probes and raw results in the ignored run directory; summarize sanitized results in PROGRESS.md.

## Completion checklist

- [ ] Inspect production code, callers, guards, contracts and tests.
- [ ] Save both independent investigations before evidence exchange.
- [ ] Record collaboration and independent candidate dispositions.
- [ ] Execute applicable reproductions and record limits.
- [ ] Check source drift and update PROGRESS.md with evidence.
