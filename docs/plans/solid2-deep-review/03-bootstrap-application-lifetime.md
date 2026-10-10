# 03 Bootstrap and application lifetime

Initial status: pending. Current status is recorded only in [PROGRESS.md](PROGRESS.md).

## Source scope

src/app; runtime owners; platform window adapter. Expanded paths are in the run coverage record.

## Relevant documentation

architecture/system-overview.md; frontend/architecture-and-ui-conventions.md. Paths are relative to docs unless explicitly named as skill records. Read linked contracts needed by the investigation.

## Investigation questions

Trace mount, initialization, view switching, startup, lazy imports, retry, HMR and window controls.

## Reproduction scenarios

Dispose during startup/listener registration; reject lazy import and retry; switch settings/gallery while initialization is pending.

## Required evidence

Two independent reviewer identities and perspectives, inspected paths, commands with results, callers and guards, saved initial findings before collaboration, response-order or runtime evidence where relevant, source hashes and independent candidate verdicts. Keep all probes and raw results in the ignored run directory; summarize sanitized results in PROGRESS.md.

## Completion checklist

- [ ] Inspect production code, callers, guards, contracts and tests.
- [ ] Save both independent investigations before evidence exchange.
- [ ] Record collaboration and independent candidate dispositions.
- [ ] Execute applicable reproductions and record limits.
- [ ] Check source drift and update PROGRESS.md with evidence.
