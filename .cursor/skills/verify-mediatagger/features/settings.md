# Settings navigation

Settings exposes appearance, indexed scan paths, thumbnail actions, and data operations.

## Sub-features

- `settings-open`: open settings from the gallery toolbar.
- `settings-roots`: inspect the temporary scan path.
- `settings-back`: return to the populated gallery.

## How to get to it (user POV)

Click the gear button in the gallery. Click Back to return.

## Driving it with WebdriverIO

Preconditions: gallery open and doctor succeeds.

```sh
bun run app:control click "$control_session" 'button[aria-label="Open settings"]'
bun run app:control inspect "$control_session" h2
bun run app:control screenshot "$control_session"
bun run app:control click "$control_session" 'button[aria-label="Back"]'
bun run app:control inspect "$control_session" .filter-input
```

Require `Appearance`, `Scan settings`, and the run's temporary media path in the returned
text. Require the search field after Back. This recipe proves navigation, not destructive actions.

## Gotchas

Do not use Choose folder or import/export buttons as a way to reach personal files. Their
native dialogs are outside this controller's selector support. Existing desktop workflow
tests cover temporary data operations separately.
