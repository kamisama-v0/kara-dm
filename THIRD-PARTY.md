# Third-party software

Kara Download Manager's own code is Apache-2.0 (see [LICENSE](LICENSE)). Binary releases also ship the
following separately licensed components. They are unmodified upstream builds; their licenses apply to them,
not to Kara Download Manager.

## Programs started by Kara Download Manager

| Component | Used for | License | Source |
| --- | --- | --- | --- |
| [aria2](https://aria2.github.io/) 1.37.0 | download engine (run as a separate process, JSON-RPC on 127.0.0.1). Windows: the official `aria2-1.37.0-win-64bit-build1` binary, its license files are in `licenses/aria2/` | GPL-2.0-or-later with OpenSSL exception | <https://github.com/aria2/aria2/releases/tag/release-1.37.0> |
| [yt-dlp](https://github.com/yt-dlp/yt-dlp) 2026.08.19 | HLS/DASH streams and "this page's video" | Unlicense | <https://github.com/yt-dlp/yt-dlp/releases/tag/2026.08.19> |
| [FFmpeg](https://ffmpeg.org/) 8.1.3 | merging video + audio streams (YouTube via yt-dlp, DASH). Flatpak: built from source. Windows: the [BtbN](https://github.com/BtbN/FFmpeg-Builds) LGPL shared build `ffmpeg-n8.1.3-18-g46d899d7a0-win64-lgpl-shared-8.1` (release `autobuild-2026-10-19-13-06`) in `ffmpeg\`, license in `licenses/ffmpeg/` | LGPL-2.1-or-later (built without `--enable-gpl`) | <https://ffmpeg.org/releases/ffmpeg-8.1.3.tar.xz>, <https://github.com/FFmpeg/FFmpeg/commit/46d899d7a0>, build scripts <https://github.com/BtbN/FFmpeg-Builds> |

`curl.exe` comes with Windows 10 and later.

## Interface and libraries

The window is drawn by [iced](https://github.com/iced-rs/iced) (MIT), linked into `kara-dm` itself, with its
software renderer [tiny-skia](https://github.com/linebender/tiny-skia) (BSD-3-Clause) or, with "Use hardware
acceleration", [wgpu](https://github.com/gfx-rs/wgpu) (MIT OR Apache-2.0). No GTK or other GUI libraries ship with
Kara DM. On Linux it uses the system's Wayland/X11, xkbcommon and (for hardware acceleration) Vulkan/OpenGL
drivers; on Windows only system DLLs.

## Rust crates

Statically linked Rust dependencies (iced, wgpu, tiny-skia, cosmic-text, winit, window-vibrancy, notify-rust,
rfd, opener, dirs, chrono, tungstenite, ureq, serde, …) are mostly MIT and/or Apache-2.0; a few are BSD-2/3-Clause,
Zlib, ISC, BSL-1.0, Unicode-3.0, CC0-1.0 or MPL-2.0 (`option-ext`, unmodified). `cargo tree --format "{p} {l}"`
lists every crate with its license.

If you need the exact corresponding source of a bundled GPL/LGPL component for a given release, open an issue
and we will provide it.
