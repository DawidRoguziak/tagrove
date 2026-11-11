# 0023: app:control reports an unresponsive app instead of failing, allows only native Escape there, and runs stop outside the command queue

- Status: Accepted
- Date: 2026-11-11
- Areas: testing, agent tooling, security surface
- Related: [testing: persistent desktop control](../development/testing.md#persistent-desktop-control), [verification skill](../../.claude/skills/verify-mediatagger/SKILL.md), [ADR 0011](0011-audit-qa-data-safety.md)

## Context

`e2e/control.js` runs a doctor before every command except `stop` and `logs`. The doctor proves the window title and app-data path through IPC, then reads the scan roots. ADR 0011 added the doctor states `alert-open`, `busy-maintenance` and `tainted`, which restrict commands instead of weakening checks.

The audit run of 2026-11-08 found three ways this broke down:

- The webdriver client aborted each HTTP request after 30 s, the same as WebKitWebDriver's script timeout. The client usually lost that race, and a "Request timed out!" error was not classified as a probe timeout. `busy-maintenance` never appeared, and a native export that took more than 30 s tainted the session.
- If a save dialog is given a path in a folder that does not exist, GTK opens its own modal "The folder contents could not be displayed" dialog. That dialog blocks the app's main thread, so the window-title probe stops answering. Every doctor failed, which refused `screenshot` and `native`. The session could neither be observed nor recovered.
- `stop` waited in the same promise queue as the commands and the 2 s console polls. A hung WebView piled polls up in front of it, so `stop` timed out for its client.

## Options considered

1. **Leave the gate as it is and document "stop and restart".** No change to the safety model. Rejected: the session is lost, and so is the evidence for what the app does with the dialog. QA-14.8 could not be observed at all.
2. **Treat every probe timeout as `busy-maintenance`.** One state for every timeout. Rejected: `busy-maintenance` forbids every action, so the GTK dialog, which never closes by waiting, would still leave the session unrecoverable.
3. **Allow all native input while the identity probe does not answer.** Easiest to drive. Rejected: native input bypasses WebDriver, and the doctor after native input exists to prove identity again before anything else acts.
4. **A separate `unresponsive` state that allows observation and exactly `native key Escape`, plus a request budget above the script timeout and an out-of-queue `stop`.** Chosen.

## Decision

- The HTTP request budget is `REQUEST_TIMEOUT_MS` (35 s), 5 s above `SCRIPT_TIMEOUT_MS` (30 s), so the driver reports its script timeout first. A client "Request timed out!" is still classified as a probe timeout (`probeFailureState` in `e2e/control-support.js`).
- A probe timeout after the title and app-data path are verified gives `busy-maintenance`, which still allows observation only. A timeout of an identity probe gives `unresponsive`. That state allows observation commands, `stop`, `logs`, and exactly `native key Escape` (`isNativeDismiss`). Every other action is refused. An identity mismatch or any other probe error still throws, and after native input that still taints the session.
- Native input is still bounded by `nativeEnvironment` and `e2e/control-native.py`: the session's private display, its `XAUTHORITY`, `XDG_DATA_HOME`, and the scratch `owner` file. The doctor that follows native input runs unchanged. A `busy-maintenance` or `unresponsive` result from that doctor does not taint the session, and the next command runs the doctor again.
- `stop` runs at once, outside the command queue (`createCommandQueue`). Commands that have not started when stopping begins, or whose client has disconnected, are not run. Console polls coalesce, so at most one is queued or running. A stop that lands in the middle of a native command's doctor does not turn the stopped manifest into `tainted`.
- The 190 s socket budget (`SOCKET_TIMEOUT_MS`) covers the doctor before a command plus a 90 s `wait`, or a native command with its 20 s helper limit and its own doctor.

## Consequences

- QA can screenshot a native error dialog, dismiss it, and continue in the same session. Verified on a private Xvfb display: CSV export to a missing folder gave `unresponsive`; `native key Escape` gave `ok`; a second Escape gave "CSV export cancelled.". A 1.5 GB synthetic backup export confirmed with `native key Return` gave `busy-maintenance` without tainting the session.
- While a probe hangs, each command that runs the doctor takes about 30 s.
- One unverified-identity action now exists: Escape on the private display. Adding any other action to the `unresponsive` gate needs a new ADR.
- Unit tests in `e2e/control-support.test.js` (`bun run test:app-control`) cover the gate, the classification, the budgets, stop outside the queue, skipped stale commands and poll coalescing.
