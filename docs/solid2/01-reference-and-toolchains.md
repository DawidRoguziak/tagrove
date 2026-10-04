# Stage 1: Reference and separate toolchains

Previous stage: none. Next: [platform and shell](02-platform-and-shell.md).
Read the [architecture and completion rules](README.md) and update [progress](PROGRESS.md).

## Outcome

Keep the working React application available while a separately compiled Solid entry
can be built, tested, and launched through the same isolated desktop harness.

## Implementation

- Record the React baseline commit, package versions, test results, and current UI states.
  Start a behavior inventory under this plan directory mapping each existing test suite
  to its future owner. Mark assertions as retained, ported, or replaced with an equivalent
  behavior check. Do not require the final test count to equal the original count.
- Recheck the newest Solid 2 RC and install an exact compatible package set. Update the
  Node requirement to `>=22.12.0`; preserve Bun and the current test runner.
- Add the Solid entry under `src/app/main.tsx`. Keep the existing React entry working.
- Select the entry and compiler at build time with
  `MEDIATAGGER_FRONTEND=react|solid`, defaulting to React. Reject unsupported values.
  The generated desktop HTML still uses the normal entry document and `dist/` output.
- Separate TypeScript contexts and Vitest projects for React and Solid. Shared pure
  tests run once. Configure the Solid test project to resolve the browser runtime,
  include the Solid compiler, and dispose mounted roots after each test.
- Preserve shared DOM mocks, controlled suggestion-worker fixtures, user-event, and
  jest-dom matchers. Await language setup where assertions depend on it.
- Pass renderer selection through the existing build and desktop commands. Preserve
  all identifier, title, profile, target-directory, and cleanup guards. Do not introduce
  a second concurrent desktop instance or a runtime UI switch.
- Capture representative synthetic UI screenshots and current gallery measurements in
  ignored artifact directories. Include both themes, English/Polish, loading/failure
  states, and relevant responsive layouts.

## Tests and acceptance

- Re-run the React frontend suite and typecheck against the recorded baseline.
- Add Solid smoke tests for initial rendering, a reactive update, asynchronous completion,
  and cleanup after disposal. Verify the test observes the browser runtime.
- Build both entries and inspect their dependency graphs for renderer separation.
- Run the existing baseline backend/desktop layers and relevant opt-in scenarios using
  temporary media. Record actual passes, skips, and environment limits separately.
- Launch each entry through the isolated desktop harness. The Solid entry only needs
  to establish working bootstrap at this stage; React remains the functional reference.

## Review and local commit

Review compiler/type isolation, test discovery, fresh frontend builds, profile guards,
and baseline reproducibility. Apply the complete stage gate from the README.

Intended commit: `build: add isolated Solid 2 migration setup`.
