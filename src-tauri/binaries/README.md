# Windows media sidecars

Windows packages use the target-specific executables below. The executables stay ignored because of their size, while this file records the reviewed input and expected output hashes.

## Pinned release

- Provider: Gyan Doshi's Windows builds of FFmpeg, <https://www.gyan.dev/ffmpeg/builds/>
- Upstream project: FFmpeg, <https://ffmpeg.org/>
- Build: `ffmpeg-8.0.1-full_build` from the Gyan.dev release archive
- Architecture: Windows x86-64
- License: GPLv3 build; retain the license files distributed in the archive when preparing release materials
- Retrieved release archive: <https://www.gyan.dev/ffmpeg/builds/packages/ffmpeg-8.0.1-full_build.7z>

Copy `bin/ffmpeg.exe` and `bin/ffprobe.exe` from that archive to:

- `ffmpeg-x86_64-pc-windows-msvc.exe`
- `ffprobe-x86_64-pc-windows-msvc.exe`

Do not rename the application identifiers or these target-triple suffixes. Tauri derives the packaged sidecar names from `src-tauri/tauri.windows.conf.json`.

## Verification

Run from `src-tauri/binaries` in PowerShell:

```powershell
Get-FileHash -Algorithm SHA256 .\ffmpeg-x86_64-pc-windows-msvc.exe
Get-FileHash -Algorithm SHA256 .\ffprobe-x86_64-pc-windows-msvc.exe
Get-Content .\SHA256SUMS
```

Both hashes must match `SHA256SUMS`. Also run each executable with `-version` and confirm that the first line reports `8.0.1-full_build-www.gyan.dev`. Update this file and `SHA256SUMS` together when upgrading the pair, then smoke-test an installed MSI or NSIS package without a system FFmpeg on `PATH`.
