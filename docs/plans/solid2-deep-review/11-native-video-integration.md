# 11 Native video integration

Initial status: pending. Current status is recorded only in [PROGRESS.md](PROGRESS.md).

## Source scope

src/features/lightbox/video; platform video; Rust video player/events/surface. Expanded paths are in the run coverage record.

## Relevant documentation

subsystems/lightbox.md; architecture/ipc-contract.md; architecture/system-overview.md. Paths are relative to docs unless explicitly named as skill records. Read linked contracts needed by the investigation.

## Investigation questions

Check native session identity, late open/control/close, bounds, listeners, media switching and exit-animation teardown.

## Reproduction scenarios

30 reopen cycles; stale session controls; disposal during open; distinguish worker/session assertions from decoded pixels.

## Required evidence

Two independent reviewer identities and perspectives, inspected paths, commands with results, callers and guards, saved initial findings before collaboration, response-order or runtime evidence where relevant, source hashes and independent candidate verdicts. Keep all probes and raw results in the ignored run directory; summarize sanitized results in PROGRESS.md.

## Completion checklist

- [ ] Inspect production code, callers, guards, contracts and tests.
- [ ] Save both independent investigations before evidence exchange.
- [ ] Record collaboration and independent candidate dispositions.
- [ ] Execute applicable reproductions and record limits.
- [ ] Check source drift and update PROGRESS.md with evidence.
