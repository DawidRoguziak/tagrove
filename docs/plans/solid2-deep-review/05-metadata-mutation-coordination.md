# 05 Metadata and mutation coordination

Initial status: pending. Current status is recorded only in [PROGRESS.md](PROGRESS.md).

## Source scope

src/library/metadata, createLibrary, vocabulary; runtime mutation callers. Expanded paths are in the run coverage record.

## Relevant documentation

subsystems/search-tags-and-media-groups.md; subsystems/data-safety-and-portability.md; architecture/ipc-contract.md. Paths are relative to docs unless explicitly named as skill records. Read linked contracts needed by the investigation.

## Investigation questions

Check details, canonical tags, ownership tokens, barriers, optimistic patches, partial responses and identity resets.

## Reproduction scenarios

Overlap lightbox and bulk saves with maintenance; reverse completions; evict details then save; return partial canonical results.

## Required evidence

Two independent reviewer identities and perspectives, inspected paths, commands with results, callers and guards, saved initial findings before collaboration, response-order or runtime evidence where relevant, source hashes and independent candidate verdicts. Keep all probes and raw results in the ignored run directory; summarize sanitized results in PROGRESS.md.

## Completion checklist

- [ ] Inspect production code, callers, guards, contracts and tests.
- [ ] Save both independent investigations before evidence exchange.
- [ ] Record collaboration and independent candidate dispositions.
- [ ] Execute applicable reproductions and record limits.
- [ ] Check source drift and update PROGRESS.md with evidence.
