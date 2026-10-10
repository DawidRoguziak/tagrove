# 10 Lightbox model and image controls

Initial status: pending. Current status is recorded only in [PROGRESS.md](PROGRESS.md).

## Source scope

src/features/lightbox except native video internals; assets commands. Expanded paths are in the run coverage record.

## Relevant documentation

subsystems/lightbox.md; architecture/ipc-contract.md. Paths are relative to docs unless explicitly named as skill records. Read linked contracts needed by the investigation.

## Investigation questions

Review navigation, details, pending edits, close/reopen, deletion/rename, image errors, pan/zoom, fullscreen and keys.

## Reproduction scenarios

Rapid navigation with pending responses; close during save/delete; image failure/retry; verify target and persisted identity.

## Required evidence

Two independent reviewer identities and perspectives, inspected paths, commands with results, callers and guards, saved initial findings before collaboration, response-order or runtime evidence where relevant, source hashes and independent candidate verdicts. Keep all probes and raw results in the ignored run directory; summarize sanitized results in PROGRESS.md.

## Completion checklist

- [ ] Inspect production code, callers, guards, contracts and tests.
- [ ] Save both independent investigations before evidence exchange.
- [ ] Record collaboration and independent candidate dispositions.
- [ ] Execute applicable reproductions and record limits.
- [ ] Check source drift and update PROGRESS.md with evidence.
