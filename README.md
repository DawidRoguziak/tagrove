# Tagrove

Tagrove is a desktop application for browsing and tagging large local collections of images, GIFs and videos. Organize files with tags, favorites and media groups, search the library and play videos without moving your collection into a cloud service.

The public publisher is **Tagrove**. The application uses Tauri 2, Rust, SQLite, React and TypeScript. Linux video playback uses libmpv, with ffmpeg and ffprobe for media processing.

## Build and development

See [setup and build instructions](docs/development/setup-and-build.md), [system architecture](docs/architecture/system-overview.md) and [testing](docs/development/testing.md). Use Bun for frontend dependencies and `bun run tauri:dev` for desktop development. Development and tests use separate data profiles.

## License and privacy

Tagrove application code is licensed under [GPL-3.0-or-later](LICENSE). You may modify and redistribute it under that license. It comes without warranty to the extent permitted by law. Third-party components retain their own licenses and author notices; see [licenses and corresponding sources](docs/development/linux-packaging.md#licenses-and-corresponding-sources).

Tagrove can change and permanently delete files. Back up your media and library before making changes. Read the [privacy notice](PRIVACY.md) for local storage, exports and external links. The application also includes the license and privacy notice offline in Settings → About.

## Code and contact

The [GitHub repository](https://github.com/DawidRoguziak/tagrove) hosts the source. Contact Tagrove or report a problem through [GitHub issues](https://github.com/DawidRoguziak/tagrove/issues). Use synthetic examples and remove private paths, media and credentials from reports.
