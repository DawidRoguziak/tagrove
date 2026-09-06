# Tagrove Flathub preparation notes

These are generated technical notes for the maintainer, not a submission PR
description. Flathub currently requires the human maintainer to author and open
submission interactions and disclose generated application or packaging material.

Tagrove browses and tags local image, GIF and video collections. It uses React
with Tauri, GTK/WebKit and libmpv. Application code is GPL-3.0-or-later.

The manifest targets x86_64 and GNOME 50. Frontend and Rust dependencies come
from the release lockfiles and compile without network access. libmpv, ffmpeg
and ffprobe are built from pinned sources. GTK, WebKit and graphics support
come from the runtime.

Home and removable-media access support scanning collections in place.
Writable access supports user-requested rename/delete operations. Wayland,
fallback X11, IPC, graphics devices and PulseAudio support the desktop and
native video player.

Before submitting, insert the chosen app ID, public repository, release archive
and digest, developer identity, screenshot links, and links to the actual
package-verification results. Confirm ownership of the app ID and source
availability for the bundled dependencies. Run a full build and linter against
the actual public release before submitting.

The packaging implementation and these notes were produced with an AI coding
assistant. Review and disclose that assistance according to Flathub's current
submission policy. Include the tests actually run and any remaining limitations.
