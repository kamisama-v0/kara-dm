# Third-party software

Kara Download Manager's own code is Apache-2.0 (see [LICENSE](LICENSE)). Binary releases also ship the
following separately licensed components. They are unmodified upstream builds; their licenses apply to them,
not to Kara Download Manager.

## Programs started by Kara Download Manager

| Component | Used for | License | Source |
| --- | --- | --- | --- |
| [aria2](https://aria2.github.io/) 1.37.0 | download engine (run as a separate process, JSON-RPC on 127.0.0.1). Windows: the official `aria2-1.37.0-win-64bit-build1` binary, its license files are in `licenses/aria2/` | GPL-2.0-or-later with OpenSSL exception | <https://github.com/aria2/aria2/releases/tag/release-1.37.0> |
| [yt-dlp](https://github.com/yt-dlp/yt-dlp) 2026.08.19 | HLS/DASH streams and "this page's video" | Unlicense | <https://github.com/yt-dlp/yt-dlp/releases/tag/2026.08.19> |
| [FFmpeg](https://ffmpeg.org/) 8.1.3 (Flatpak only) | merging video + audio streams | LGPL-2.1-or-later (built without `--enable-gpl`) | <https://ffmpeg.org/releases/ffmpeg-8.1.3.tar.xz> |

On Windows, ffmpeg is not bundled; install it separately (e.g. `winget install Gyan.FFmpeg`) to merge
separate video/audio streams. `curl.exe` comes with Windows 10 and later.

## Libraries (dynamically linked)

| Component | License | Source |
| --- | --- | --- |
| GTK 4 | LGPL-2.1-or-later | <https://gitlab.gnome.org/GNOME/gtk> |
| libadwaita | LGPL-2.1-or-later | <https://gitlab.gnome.org/GNOME/libadwaita> |
| GLib, GIO | LGPL-2.1-or-later | <https://gitlab.gnome.org/GNOME/glib> |
| Pango, gdk-pixbuf, librsvg, cairo, HarfBuzz, and their dependencies | LGPL / MPL / MIT-style (see each project) | Windows builds come from [MSYS2](https://packages.msys2.org/) UCRT64 packages; each package page links its exact source |
| Adwaita icon theme | CC-BY-SA-3.0 / LGPL-3.0 | <https://gitlab.gnome.org/GNOME/adwaita-icon-theme> |

On Linux (Flatpak) these libraries come from the GNOME runtime and are not part of the Kara DM download.

## Rust crates

Statically linked Rust dependencies (gtk-rs, tungstenite, ureq, serde, …) are MIT and/or Apache-2.0 licensed.
`cargo tree --format "{p} {l}"` lists them with their licenses.

If you need the exact corresponding source of a bundled GPL/LGPL component for a given release, open an issue
and we will provide it.
