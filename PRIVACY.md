# Privacy

Tagrove is a desktop application published by Tagrove for browsing and tagging local media.

## Data on your device

Tagrove stores its library in a local SQLite database in its application data directory. The library includes media paths, file metadata, tags, favorites, media groups and scan folders. Generated thumbnails are stored locally. Language and appearance preferences are kept in the local WebView storage. Development, E2E and release builds use separate data profiles; Flatpak also uses its own application directory.

Media stays in the folders you select. Tagrove reads these files to index, display and play them. Some actions can rename or permanently delete source files. Keep backups of both your media and library before making changes.

## Network activity

Review of the application code found no telemetry, analytics or automatic uploading of media or library data. Tagrove does not require an account or a cloud service. Fonts, the license and this privacy notice are bundled for offline use. The Flatpak application has no network permission.

Your operating system or other software may synchronize folders you choose, including export destinations. Those services operate separately from Tagrove.

## Exports and support

CSV exports and library backups run only when you request them and are written to your chosen destination. Exports can contain file paths, tags and other library metadata; backups can also contain thumbnails. Review exported data before sharing it.

Source code and contact links open the project repository in your system browser after a click. GitHub then handles the visit under its own privacy policy and account settings. Opening a link does not attach your library or media. Anything you submit in a public issue may be visible to others. Avoid posting private media, database files, credentials or logs containing personal paths.

Contact Tagrove through the project repository: https://github.com/DawidRoguziak/tagrove
