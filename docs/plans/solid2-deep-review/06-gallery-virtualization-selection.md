# 06 Gallery virtualization and selection

Initial status: pending. Current status is recorded only in [PROGRESS.md](PROGRESS.md).

## Source scope

src/features/gallery, query page access, selection consumers. Expanded paths are in the run coverage record.

## Relevant documentation

subsystems/library-query-and-gallery.md; frontend/architecture-and-ui-conventions.md. Paths are relative to docs unless explicitly named as skill records. Read linked contracts needed by the investigation.

## Investigation questions

Review measurement, row identity, overscan, resize anchors, range selection, drag geometry and remount behavior.

## Reproduction scenarios

Existing 10,000/26,000-slot tests; metadata-only 100,000/1,000,000 entries; inspect mounted nodes and request work.

## Required evidence

Two independent reviewer identities and perspectives, inspected paths, commands with results, callers and guards, saved initial findings before collaboration, response-order or runtime evidence where relevant, source hashes and independent candidate verdicts. Keep all probes and raw results in the ignored run directory; summarize sanitized results in PROGRESS.md.

## Completion checklist

- [ ] Inspect production code, callers, guards, contracts and tests.
- [ ] Save both independent investigations before evidence exchange.
- [ ] Record collaboration and independent candidate dispositions.
- [ ] Execute applicable reproductions and record limits.
- [ ] Check source drift and update PROGRESS.md with evidence.
