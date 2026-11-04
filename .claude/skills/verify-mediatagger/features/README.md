# MediaTagger verification map

Start the controller and run doctor as described in [the skill](../SKILL.md). Use the session
ID printed by `READY` as `control_session`. All fixtures and writes must remain temporary.
Read the feature's gotchas before acting. Preserve action logs and screenshots after cleanup.

- [Gallery and search](gallery-search.md): search submission, empty results, clear, and tiles.
- [Settings](settings.md): navigation, scan-path visibility, and return to the gallery.
- [Lightbox](lightbox.md): image viewing, keyboard close, tags, and native-media limitations.

The map covers these entry points only. Native file dialogs and GTK playback controls are
unsupported by this WebDriver controller. Do not claim them as verified through an IPC call.

For each proof, record the action and resulting state with `inspect` and `screenshot`. For
a persistent change, reopen the affected UI or read the temporary database as well. Follow
the owning subsystem documentation for broader test coverage.
