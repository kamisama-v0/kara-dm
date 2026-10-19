# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/).

## [Unreleased]

### Changed
- Browser extension 0.1.3: first listed (public) version on addons.mozilla.org; same code as 0.1.2.

## [0.2.0] - 2026-10-20

First public release.

### Added
- Windows 10/11 x64 build (beta): installer without admin rights (optional start at sign-in) and portable zip,
  bundling aria2 and yt-dlp.
- Linux Flatpak (GNOME 49 runtime) bundling aria2, ffmpeg and yt-dlp; asks the Background portal to start at
  login.
- Kara DM browser extension 0.1.2 for Firefox and Chromium: catches downloads via the downloads API and leaves
  them to the browser when the app is not running, glass "Download video" button, popup, context menu, options.
- New logo and app icon.
- `--version` and `--selftest` command-line options.
- Automatic retry with fewer connections when a server answers HTTP 429/503.
- File names are made safe for Windows (reserved characters and device names).

### Changed
- App ID is now `io.github.kamisama_v0.KaraDM`.
- Default download folder follows the system Downloads folder.
- Licensed under Apache-2.0.

## [0.1.0] - 2026-10-19

### Added
- Initial Linux version: aria2 engine, compatible with the Neat Download Manager browser extension, HLS via
  yt-dlp, DASH merge via ffmpeg, POST downloads via curl, progress window with segment bar, history and
  categories.

[Unreleased]: https://github.com/kamisama-v0/kara-dm/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/kamisama-v0/kara-dm/releases/tag/v0.2.0
