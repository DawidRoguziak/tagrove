# Settings navigation

Settings exposes appearance, language, indexed scan paths, thumbnail actions, and data operations.

## Sub-features

- `settings-open`: open settings from the gallery toolbar.
- `settings-roots`: inspect the temporary scan path.
- `settings-back`: return to the populated gallery.
- `settings-language`: switch the language with `select` and restore English.
- `settings-choose-folder`: add the media root through the native folder picker.
- `settings-maintenance`: observe long operations through the doctor states.

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

Require `Appearance`, `Language`, `Scan settings`, and the run's temporary media path in the returned
text. Require the search field after Back. This recipe proves navigation, not destructive actions.

Language (`#settings-language-select`, 11 languages) is a `<select>`; sessions start in English.
Switch back to `en` at the end of a recipe:

```sh
bun run app:control select "$control_session" '#settings-language-select' value=pl
bun run app:control inspect "$control_session" '#settings-language'
bun run app:control select "$control_session" '#settings-language-select' value=en
```

Choose folder opens a native GTK dialog. Drive it with `native`, typing exactly the media root
that `doctor` prints and never another folder:

```sh
bun run app:control click "$control_session" 'button=Choose folder'
bun run app:control screenshot "$control_session"
bun run app:control native "$control_session" key Control+l
bun run app:control native "$control_session" key Control+a
bun run app:control native "$control_session" type /media/root/printed/by/doctor
bun run app:control native "$control_session" key Return
bun run app:control wait "$control_session" idle 60000
```

Cancel a picker with `native key Escape`. The CSV and backup import/export buttons open the
same kind of dialog; point them only at files inside the session's temporary root, and save exports under `<scratch>/export/` (created by the controller), because the app refuses a target inside the indexed media root. A typed save
path in a folder that does not exist opens GTK's own error dialog over the picker; doctor then
reports `unresponsive` until `native key Escape` dismisses it (see the skill's native input
section).

Long operations: Rescan all, Remove root, Clear library, and backup export/import hold locks
without a time limit. While one runs, `doctor` can report `busy-maintenance` (a probe timed
out): `inspect`, `wait`, `screenshot`, and `logs` still work and actions are refused. Poll with
`wait ... idle <ms>` and `logs` until doctor says `ok`. A deleted file in the lightbox may
leave a `window.alert` (`alert-open`: use `alert text`, then `alert accept`). A failed doctor
after `native` sets `tainted`, which allows only `logs`, `screenshot`, and `stop`. `stop`
always works, even while another command hangs.

Confirmation dialogs (`#remove-scan-root-confirm-heading`, `#clear-library-confirm-heading`,
`#import-db-overwrite-confirm-heading`, `#duplicate-delete-confirm-heading`) are modals:
Escape closes the top one. Use them only with disposable media.

## Gotchas

Do not use Choose folder or import/export buttons as a way to reach personal files. Their
native dialogs are driven with `native` and must stay inside the session's temporary root.
Existing desktop workflow tests cover temporary data operations separately.
