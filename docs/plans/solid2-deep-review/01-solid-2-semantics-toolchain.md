# 01 Solid 2 semantics and toolchain

Initial status: pending. Current status is recorded only in [PROGRESS.md](PROGRESS.md).

## Source scope

package.json, bun.lock, Vite/TypeScript configuration; installed Solid runtime/compiler; all reactive owners. Expanded paths are in the run coverage record.

## Relevant documentation

frontend/architecture-and-ui-conventions.md; development/setup-and-build.md. Paths are relative to docs unless explicitly named as skill records. Read linked contracts needed by the investigation.

## Investigation questions

Check pinned runtime, renderer/compiler compatibility, JSX and build graph. Trace batched reads/writes, compute/apply effects, reactive props and owner cleanup against installed RC.

## Reproduction scenarios

Repeated commands in one turn, writes followed by reads, dispose before effect application; production build module graph.

## Required evidence

Two independent reviewer identities and perspectives, inspected paths, commands with results, callers and guards, saved initial findings before collaboration, response-order or runtime evidence where relevant, source hashes and independent candidate verdicts. Keep all probes and raw results in the ignored run directory; summarize sanitized results in PROGRESS.md.

## Completion checklist

- [ ] Inspect production code, callers, guards, contracts and tests.
- [ ] Save both independent investigations before evidence exchange.
- [ ] Record collaboration and independent candidate dispositions.
- [ ] Execute applicable reproductions and record limits.
- [ ] Check source drift and update PROGRESS.md with evidence.
