# MediaTagger verification map

Start the controller and run doctor as described in [the skill](../SKILL.md). Use the session
ID printed by `READY` as `control_session`. All fixtures and writes must remain temporary.
Read the feature's gotchas before acting. Preserve action logs and screenshots after cleanup.

- [Gallery and search](gallery-search.md): search submission, empty results, clear, tiles, timeline, selection, and tile size.
- [Settings](settings.md): navigation, scan-path visibility, language, native folder picker, and long operations.
- [Lightbox](lightbox.md): image viewing, keyboard close, tags, native video controls, and delete.

The map covers these entry points only. Native file dialogs and GTK playback controls are
driven with `native` (XTest on the private display, coordinates from a screenshot); no
arbitrary IPC is exposed, so do not claim them as verified through an IPC call.

Controller commands beyond click/fill/keys: `wait ... visible|hidden` (tile loader), `drag`
(timeline scrubber, rectangle selection), `modclick` (Shift/Ctrl selection), `wheel ... ctrl`
(tile size), `select` (language), `alert` (delete alert), `resize`, and `native`. Doctor states
beyond `ok`: `alert-open`, `busy-maintenance`, `unresponsive`, `tainted`.

For each proof, record the action and resulting state with `inspect` and `screenshot`. For
a persistent change, reopen the affected UI or read the temporary database as well. Follow
the owning subsystem documentation for broader test coverage.
