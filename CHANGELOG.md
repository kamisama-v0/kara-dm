# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/).

## [Unreleased]

### Changed
- New interface built with iced instead of GTK 4 / libadwaita: glass panels over the system blur (Mica on
  Windows 11), toolbar, categories, download list, progress windows with the segment bar, settings and menus.
  Memory use on Windows dropped from about 125 MB (background) / 210 MB (window open) to about 3 MB / 9 MB, and the
  Windows bundle no longer carries 65 MB of GTK libraries. No MSYS2 is needed to build on Windows.
- Settings → "Use hardware acceleration" draws the window with the GPU (wgpu) instead of the software renderer
  (tiny-skia). Off by default: the GPU path uses about 200 MB; it takes effect after "Save and restart".
- Single-instance handling (a second launch opens the window, `--quit` stops the running app) now works the same
  way on Linux and Windows.
- yt-dlp downloads (YouTube and other page videos) are named after the video's own title plus the resolution
  actually downloaded, as a dash-separated slug: "niki - Tsunami" at 1080p becomes `niki-Tsunami-1080.webm`,
  audio only `niki-Tsunami-audio.m4a`; the name shows in the list as soon as
  the format is chosen.
- Windows: FFmpeg 8.1.3 (LGPL build) is bundled in `ffmpeg\`, so YouTube video and audio are merged into one file.

### Fixed
- Google Drive downloads of large files ended "complete" as a 2 KB `download` page. The browser extension (0.1.5)
  now passes the exact Cookie header the browser sent for the download, and otherwise only that site's
  first-party cookies; it used to mix in partitioned copies of Google's cookies from other sites, so Google saw a
  different session and refused the one-time download token. If Drive still answers with its virus-scan warning,
  Kara DM follows the page's form with the fresh token; other Drive pages (e.g. "Quota exceeded") now show as a
  failed download with Google's message instead of a saved HTML file.
- Browser extension: the media list no longer shows a site's sound effects (e.g. `success.mp3`, served without a
  size), and media is labelled simply "Video" or "Audio" (audio used to show as "MP3 video").

### Changed
- The desktop app is in English (menus, settings, progress window, notifications, messages and categories).
- The extension's video menu and popup show the video's title instead of "This page's video".
- Audio only prefers `.m4a`, so it lands in the Music category and plays everywhere.
- Browser extension: the whole interface (popup, video button menu, settings, "Download links" page, right-click
  menu and notifications) is in English.
- Windows: YouTube videos above 360p were saved as two separate files (`.fNNN.mp4` video and `.fNNN.webm` audio)
  because ffmpeg was missing.
- Windows: clicking the Kara DM icon while it runs in the background opened the window behind other windows.
- When port 10007 was still in use at start (e.g. Neat Download Manager), the browser extension never connected
  and every download was left to the browser; Kara DM now retries every 5 seconds and takes the port over once
  it is free.

## [0.3.1] - 2026-10-20

### Added
- Update check: the app looks for a newer GitHub release shortly after start and once a day (can be turned off in
  Settings → Updates), and from the menu with "Check for updates". A banner shows the new version. On Windows,
  "Update now" downloads the installer from the release, checks its SHA-256 against the one GitHub records,
  installs it silently and reopens Kara DM. On Linux the banner opens the release page.
  Versions before 0.3.1 do not have this yet, so install 0.3.1 itself manually once.

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

[Unreleased]: https://github.com/kamisama-v0/kara-dm/compare/v0.3.1...HEAD
[0.3.1]: https://github.com/kamisama-v0/kara-dm/compare/v0.3.0...v0.3.1
[0.3.0]: https://github.com/kamisama-v0/kara-dm/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/kamisama-v0/kara-dm/releases/tag/v0.2.0
