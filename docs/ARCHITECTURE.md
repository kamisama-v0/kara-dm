# Architecture

```
┌──────────── browser ────────────┐            ┌──────────────── kara-dm (one process) ────────────────┐
│ Kara DM extension (or NDM's)    │  ws://     │ ws.rs        WebSocket server, origin check            │
│  background: downloads.onCreated├──127.0.0.1─► protocol.rs  parse message → Request                   │
│  content: "Download video" pill │   :10007   │ engine.rs    choose engine, items, history, events     │
│  popup / options                │            │ gui.rs       iced windows (daemon; tray on Windows)    │
└─────────────────────────────────┘            │ aria2.rs ──► aria2c (child, JSON-RPC 127.0.0.1:random) │
                                               │ jobs.rs  ──► yt-dlp · curl · ffmpeg (child processes)  │
                                               │ platform.rs  folders, private files, tools, notify     │
                                               └────────────────────────────────────────────────────────┘
```

## Process model

- kara-dm is a single process. `instance.rs` makes it single instance on every OS: a second launch forwards its
  arguments over `127.0.0.1:10008` and exits. The interface is an iced daemon: closing the window leaves the
  process running with no window (about 3 MB) so the browser can hand over downloads; `kara-dm --quit` stops it.
- `engine.rs` owns all state. Background threads (WebSocket connections, aria2 polling, job output readers) never
  touch it directly: they send `Event`s through an `async_channel` that `Engine::tick` drains, called by the UI
  every 125 ms (250 ms with no window). The engine asks the UI for windows and messages through `Effect`s.
- The renderer is chosen at start: tiny-skia (software, about 10 MB) by default, wgpu with "Use hardware
  acceleration".
- aria2 runs as a child process with `--stop-with-process`, a random RPC port and a random token stored in a
  private config file. A watcher restarts it if it dies. Every second the app reads `tellActive`,
  `tellWaiting` and `tellStopped` in one `system.multicall`.

## Data

| File | Content |
| --- | --- |
| `<config>/kara-dm/settings.json` | settings |
| `<data>/kara-dm/history.json` | download list (no cookies or headers) |
| `<data>/kara-dm/aria2.session` | aria2's own queue of unfinished downloads (resume after restart) |
| `<data>/kara-dm/aria2.log` | aria2 warnings |
| `<cache>/kara-dm/parts/<id>/` | separate video/audio streams before merging |
| `<runtime>/kara-dm/` | aria2 config with RPC token; temporary cookie/body files for jobs |

`<config>`, `<data>`, `<cache>`, `<runtime>` are the XDG directories on Linux (sandboxed under
`~/.var/app/…` in Flatpak), `%LOCALAPPDATA%` on Windows.

## Download states

`Waiting → Active → Complete`, with `Paused`, `Error` and `Merging` (DASH). aria2 items map their GID status;
yt-dlp/curl items follow their process. A download that fails with HTTP 429/503 is resumed with half the
connections (up to four times). Errors and pauses keep partial data; *Resume* re-adds the URL with the same
output file so aria2 continues from its `.aria2` control file.

## Extension

Manifest V3, one source tree (`extension/src`) built into `dist/firefox` (event page) and `dist/chromium`
(service worker) by `build.mjs`. Pure logic lives in `common.js` and is unit-tested with `node --test`. The
in-page button is a closed Shadow DOM so page styles cannot leak in or out.
