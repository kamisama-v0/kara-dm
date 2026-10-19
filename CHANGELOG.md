# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/).

## [Unreleased]

### Changed
- Unfinished downloads are kept in a `temp` subfolder of the download folder (including aria2 `.aria2` control
  files, yt-dlp `.part` files and DASH parts) and moved into the download folder when complete, renamed
  `name (1).ext` if the name is taken. The `temp` folder follows the download folder when you change it.
  Downloads started by older versions finish where they are.

### Fixed
- On Windows, DASH video and audio parts were stored in the Temporary Internet Files folder on C:, which Disk
  Cleanup may empty; they now stay next to the download.

## [0.3.0] - 2026-10-20

### Added
- "Download all links" in the browser extension (popup or page context menu): lists the page's links grouped by
  file type (video, audio, images, documents, archives, programs) with search and checkboxes, and sends the chosen
  files to the app in one go, without a progress window per file.
- Video quality choice for page videos (YouTube, Vimeo, …), HLS and DASH: best, 2160p down to 360p, or audio only.
  Set it in the extension popup, the video button menu or the options; the app passes it to yt-dlp.
- Two extension-only protocol keys: `10` (yt-dlp quality) and `11` (part of a batch). Neat Download Manager
  ignores them, and older Kara DM versions keep downloading at the best quality.
- The Firefox extension is published on addons.mozilla.org; each release submits the new extension version for
  review.

### Changed
- Browser extension 0.1.4.

### Fixed
- Extension popup rows that should be hidden (such as "turn off on this site" on non-web pages) are hidden again.

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

[Unreleased]: https://github.com/kamisama-v0/kara-dm/compare/v0.3.0...HEAD
[0.3.0]: https://github.com/kamisama-v0/kara-dm/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/kamisama-v0/kara-dm/releases/tag/v0.2.0
