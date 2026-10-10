# 14 Integrated performance and stability

Initial status: pending. Current status is recorded only in [PROGRESS.md](PROGRESS.md).

## Source scope

Cross-feature runtime and desktop E2E; ignored synthetic probes. Expanded paths are in the run coverage record.

## Relevant documentation

development/testing.md; architecture/system-overview.md. Paths are relative to docs unless explicitly named as skill records. Read linked contracts needed by the investigation.

## Investigation questions

Cross-feature races, cold/warm browsing, large collections, repeated owners/native reopen. Separate metadata scaling from media workloads.

## Reproduction scenarios

2,048 images; 30 warmed samples with median/p95; 100 gallery/lightbox cycles, 50 settings round trips, 30 video reopen cycles. Record CPU, IPC, DOM/cache/memory where available.

## Required evidence

Two independent reviewer identities and perspectives, inspected paths, commands with results, callers and guards, saved initial findings before collaboration, response-order or runtime evidence where relevant, source hashes and independent candidate verdicts. Keep all probes and raw results in the ignored run directory; summarize sanitized results in PROGRESS.md.

## Completion checklist

- [ ] Inspect production code, callers, guards, contracts and tests.
- [ ] Save both independent investigations before evidence exchange.
- [ ] Record collaboration and independent candidate dispositions.
- [ ] Execute applicable reproductions and record limits.
- [ ] Check source drift and update PROGRESS.md with evidence.
