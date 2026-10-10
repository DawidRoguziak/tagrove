# 04 Query sessions and page caching

Initial status: pending. Current status is recorded only in [PROGRESS.md](PROGRESS.md).

## Source scope

src/library/query; platform assets; Rust asset query service and SQLite contracts. Expanded paths are in the run coverage record.

## Relevant documentation

subsystems/library-query-and-gallery.md; architecture/ipc-contract.md; subsystems/database.md. Paths are relative to docs unless explicitly named as skill records. Read linked contracts needed by the investigation.

## Investigation questions

Trace overlapping filters, superseded starts, stale sessions, eviction, positions, range loading, loading counters and disposal.

## Reproduction scenarios

Reverse response order; return stale pages; lookup evicted positions; dispose while query start is pending.

## Required evidence

Two independent reviewer identities and perspectives, inspected paths, commands with results, callers and guards, saved initial findings before collaboration, response-order or runtime evidence where relevant, source hashes and independent candidate verdicts. Keep all probes and raw results in the ignored run directory; summarize sanitized results in PROGRESS.md.

## Completion checklist

- [ ] Inspect production code, callers, guards, contracts and tests.
- [ ] Save both independent investigations before evidence exchange.
- [ ] Record collaboration and independent candidate dispositions.
- [ ] Execute applicable reproductions and record limits.
- [ ] Check source drift and update PROGRESS.md with evidence.
